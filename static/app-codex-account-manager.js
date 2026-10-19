(() => {
  "use strict";

  const ACCOUNT_KINDS = ["oauth", "api", "claude", "codebuddy", "devin"];
  const ACCOUNT_PATHS = Object.freeze({
    oauth: "/codex_oauth", api: "/codex_api", claude: "/claude_api",
    codebuddy: "/codebuddy", devin: "/devin", agy: "/agy",
    opencode: "/codex_api/opencode",
  });
  function accountKindFromUrl() {
    const aliases = {
      "/auth": "oauth", "/codex_api/oauth": "oauth",
      "/api": "api", "/llm_api": "oauth", "/codex_api/api": "api",
      "/claude": "claude", "/codex_api/claude": "claude",
      "/agy_auth": "agy",
    };
    // 兼容历史哈希：["#api", "#auth", "#oauth", "#claude"].includes(window.location.hash.toLowerCase())
    // 以及 window.location.pathname.toLowerCase() 对应路径自动切换 setAccountKind("api")
    const path = window.location.pathname.toLowerCase().replace(/\/$/, "");
    const hash = window.location.hash.toLowerCase().replace(/^#/, "/");
    for (const route of [hash, path]) {
      const kind = Object.keys(ACCOUNT_PATHS).find((key) => ACCOUNT_PATHS[key] === route);
      if (kind) return kind;
      if (aliases[route]) return aliases[route];
      if (route === "/oauth") return "oauth";
    }
    return null;
  }
  let activeKind = "oauth";
  activeKind = accountKindFromUrl() || "oauth";
  window.accountKindFromUrl = accountKindFromUrl;
  window.currentAccountPathname = () => ACCOUNT_PATHS[activeKind];
  let allAccounts = [];
  const selectedAccountIds = new Set();
  const testingAccountKeys = new Set();

  function textCell(value, className = "") {
    const cell = document.createElement("td");
    cell.textContent = value || "-";
    if (className) cell.className = className;
    return cell;
  }

  function formatCredits(value) {
    const number = Number(value || 0);
    if (!Number.isFinite(number)) return "0";
    return Number.isInteger(number) ? String(number) : number.toFixed(2);
  }

  function formatCreditsDate(timestamp) {
    const date = new Date(Number(timestamp));
    if (Number.isNaN(date.getTime())) return "";
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  }

  function creditsOf(account) {
    return account.kind === "codebuddy" ? account.credits || null : null;
  }

  // 总积分列：左侧进度条以 3000 分为固定满分基准绘制比例，便于跨账号直观对比；右侧展示 剩余 / 总量。
  const CODEBUDDY_BENCHMARK_CREDITS = 3000;

  function creditsProgressBar(ratio) {
    const bar = document.createElement("span");
    bar.className = "credits-progress";
    const fill = document.createElement("span");
    fill.className = "credits-progress-fill";
    // 剩余不为 0 时保留最小可见宽度，避免进度条看起来是空的；最高 100%。
    const percent = ratio > 0 ? Math.min(100, Math.max(3, Math.round(ratio * 100))) : 0;
    fill.style.width = `${percent}%`;
    bar.appendChild(fill);
    return bar;
  }

  function creditsTotalCell(account) {
    const credits = creditsOf(account);
    const cell = textCell("-", "mono-text credits-cell credits-total-cell");
    if (!credits) return cell;
    if (credits.ok !== true) {
      cell.textContent = credits.error ? "查询失败" : "-";
      cell.title = credits.error || "";
      return cell;
    }
    const remaining = Number(credits.totalRemaining || 0);
    const capacity = Number(credits.totalCapacity || 0);
    // 绘图采用固定标准（3000 分为总分），避免不同账号历史总量不同导致比例无法横向比较
    const ratio = Math.min(1, Math.max(0, remaining / CODEBUDDY_BENCHMARK_CREDITS));
    cell.textContent = "";
    cell.appendChild(creditsProgressBar(ratio));
    const value = document.createElement("span");
    value.className = "credits-total-value";
    value.textContent = formatCredits(remaining);
    cell.appendChild(value);
    if (credits.expired) cell.dataset.state = "expired";
    else if (credits.expiringSoon) cell.dataset.state = "expiring";
    const benchmarkPct = Math.round((remaining / CODEBUDDY_BENCHMARK_CREDITS) * 100);
    cell.title = [
      `剩余 ${formatCredits(remaining)}`,
      capacity > 0 ? `总量 ${formatCredits(capacity)}` : "",
      `3000分基准占比 ${benchmarkPct}%`,
      credits.expiringSoonRemaining ? `临期 ${formatCredits(credits.expiringSoonRemaining)}` : "",
      credits.expiredRemaining ? `已过期 ${formatCredits(credits.expiredRemaining)}` : "",
    ].filter(Boolean).join(" · ");
    return cell;
  }

  // 分类积分列：按资源包名聚合剩余积分（最多展示 3 类，完整明细放 title）。
  function simplifyCreditPackageName(name) {
    const raw = String(name || "").trim();
    if (!raw) return "其他";
    if (raw.includes("奖励")) return "奖励";
    if (raw.includes("基础")) return "基础";
    if (raw.includes("赠送")) return "赠送";
    if (raw.includes("购买") || raw.includes("付费")) return "购买";
    if (raw.includes("体验")) return "体验";
    if (raw.includes("标准")) return "标准";
    const cleaned = raw
      .replace(/CodeBuddy/gi, "")
      .replace(/个人/g, "")
      .replace(/版本/g, "")
      .replace(/积分/g, "")
      .replace(/用量/g, "")
      .replace(/平台/g, "")
      .trim();
    if (cleaned.length >= 2 && cleaned.length <= 4) return cleaned;
    if (cleaned.length > 4) return cleaned.slice(0, 4);
    return cleaned || raw;
  }

  function creditsBreakdownGroups(credits) {
    const resources = Array.isArray(credits.resources) ? credits.resources : [];
    const groups = new Map();
    for (const resource of resources) {
      const remaining = Number(resource?.remaining || 0);
      if (remaining <= 0) continue;
      const name = resource.packageName || resource.packageCode || "未分类";
      const entry = groups.get(name) || { remaining: 0, total: 0, expireAt: 0 };
      entry.remaining += remaining;
      entry.total += Number(resource.total || 0);
      const expireAt = Number(resource.expireAt || 0);
      if (expireAt && (!entry.expireAt || expireAt < entry.expireAt)) entry.expireAt = expireAt;
      groups.set(name, entry);
    }
    return [...groups.entries()].sort((left, right) => right[1].remaining - left[1].remaining);
  }

  function creditsBreakdownCell(account) {
    const credits = creditsOf(account);
    const cell = textCell("-", "credits-cell credits-breakdown-cell");
    if (!credits || credits.ok !== true) return cell;
    const groups = creditsBreakdownGroups(credits);
    if (!groups.length) return cell;
    const shown = groups
      .slice(0, 3)
      .map(([name, entry]) => `${simplifyCreditPackageName(name)}${formatCredits(entry.remaining)}`);
    if (groups.length > shown.length) shown.push(`等${groups.length}类`);
    cell.textContent = shown.join(" ");
    cell.title = groups
      .map(([name, entry]) => {
        const expireText = entry.expireAt ? `，到期 ${formatCreditsDate(entry.expireAt)}` : "";
        return `${name}：剩余 ${formatCredits(entry.remaining)} / 总量 ${formatCredits(entry.total)}${expireText}`;
      })
      .join("\n");
    return cell;
  }

  window.WebClxCodeBuddyCredits = {
    BENCHMARK_CREDITS: CODEBUDDY_BENCHMARK_CREDITS,
    creditsProgressBar,
    creditsTotalCell,
    simplifyCreditPackageName,
    creditsBreakdownCell,
  };

  // 到期列：最早到期日期 + 剩余天数；长期有效/已过期单独展示。
  function creditsExpiryCell(account) {
    const credits = creditsOf(account);
    const cell = textCell("-", "mono-text credits-cell");
    if (!credits || credits.ok !== true) return cell;
    if (credits.expired) {
      cell.textContent = "已过期";
      cell.dataset.state = "expired";
      return cell;
    }
    const soonest = Number(credits.soonestExpireAt || 0);
    if (!soonest) {
      cell.textContent = "长期有效";
      return cell;
    }
    const days = Math.max(0, Math.ceil((soonest - Date.now()) / 86_400_000));
    cell.textContent = `${formatCreditsDate(soonest)} · ${days} 天`;
    if (credits.expiringSoon) cell.dataset.state = "expiring";
    return cell;
  }

  function actionButton(label, handler, primary = false) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = primary ? "mini-button accent" : "mini-button";
    button.textContent = label;
    button.addEventListener("click", handler);
    return button;
  }

  function projectPath() {
    if (typeof state === "undefined") return "";
    return String(state.currentDirectory?.display_path || state.workspaceDir || "");
  }

  function showStatus(message, tone = "info") {
    const element = document.getElementById("codex-account-status");
    if (!element) return;
    element.hidden = !message;
    element.textContent = message;
    element.dataset.tone = tone;
  }

  async function applyAccount(account) {
    if (account.kind === "claude") return applyClaudeAccount(account);
    if (account.kind === "codebuddy") {
      if (typeof WebClxCodeBuddyManager === "object") {
        await WebClxCodeBuddyManager.switchAccount(account);
      } else {
        showStatus("CodeBuddy 管理器尚未初始化。", "warn");
      }
      return;
    }
    if (account.kind === "devin") {
      if (typeof WebClxDevinManager === "object") {
        await WebClxDevinManager.switchAccount(account);
      } else {
        showStatus("Devin 管理器尚未初始化。", "warn");
      }
      return;
    }
    showStatus(`正在切换到 ${account.name}...`, "info");
    const params = new URLSearchParams();
    const path = projectPath();
    if (path) params.set("project_path", path);
    const encodedParams = params.toString();
    const query = encodedParams ? `?${encodedParams}` : "";
    try {
      const response = await requestJson(
        `/api/auth/codex-accounts/${encodeURIComponent(account.kind)}/${encodeURIComponent(account.id)}/apply${query}`,
        { method: "PUT" },
      );
      const daemonNote = response?.codex_daemon_restarted
        ? "已重启 Codex 后台守护进程使切换生效（正在运行的 Codex 会话可能被中断）。"
        : "";
      showStatus(`已切换到 ${account.name}，新启动的 Codex 将使用该账号。${daemonNote}`, "ok");
      await loadCodexAccounts();
      if (typeof refreshAuthPanels === "function") await refreshAuthPanels();
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  async function applyClaudeAccount(account) {
    if (typeof applyClaudePreset === "function") {
      await applyClaudePreset(account.id, document.getElementById("codex-account-status"));
      await loadCodexAccounts();
      return;
    }
    showStatus("Claude 管理器尚未初始化。", "warn");
  }

  async function deleteAccount(account) {
    const confirmed = window.confirm(`确定删除 ${account.name || "该账号"} 吗？`);
    if (!confirmed) return;
    showStatus(`正在删除 ${account.name || "账号"}…`, "info");
    try {
      if (account.kind === "oauth") {
        await deleteAuthPreset(account.id, account.name, document.getElementById("codex-account-status"));
      } else if (account.kind === "api" && typeof deleteApiPreset === "function") {
        await deleteApiPreset(account.id, account.name);
      } else if (account.kind === "claude") {
        await deleteClaudePreset(account.id, account.name);
      } else if (account.kind === "devin") {
        if (typeof WebClxDevinManager === "object") {
          await WebClxDevinManager.deleteAccount(account.id);
        } else {
          throw new Error("Devin 管理器尚未初始化。");
        }
      } else {
        throw new Error("当前类型暂不支持删除。");
      }
      await loadCodexAccounts();
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  async function toggleApiAccountFavorite(account) {
    showStatus(`正在更新 ${account.name || "API 预设"} 的收藏状态…`, "info");
    try {
      const response = await requestJson(
        `/api/auth/api-presets/${encodeURIComponent(account.id)}/favorite`,
        { method: "PUT" },
      );
      const favorite = response?.preset?.favorite === true;
      allAccounts = allAccounts.map((item) => (
        item.kind === "api" && item.id === account.id ? { ...item, favorite } : item
      ));
      renderAccounts(allAccounts);
      showStatus(`${favorite ? "已收藏" : "已取消收藏"}：${account.name}`, "ok");
      if (typeof loadApiPresets === "function") await loadApiPresets();
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  function filteredAccounts(accounts) {
    if (activeKind === "agy") return [];
    if (activeKind === "codex") {
      return accounts.filter((account) => account.kind === "oauth");
    }
    return accounts.filter((account) => account.kind === activeKind);
  }

  function orderCodexAccountsByPresetTables(accounts, apiPresets) {
    const source = Array.isArray(accounts) ? accounts : [];
    const oauthAccounts = source.filter((account) => account.kind === "oauth");
    const apiAccounts = source.filter((account) => account.kind === "api");
    const apiById = new Map(apiAccounts.map((account) => [account.id, account]));
    const orderedApiAccounts = (Array.isArray(apiPresets) ? apiPresets : [])
      .map((preset) => apiById.get(preset.id))
      .filter(Boolean);
    const orderedApiIds = new Set(orderedApiAccounts.map((account) => account.id));
    const remainingApiAccounts = apiAccounts.filter((account) => !orderedApiIds.has(account.id));
    const remainingAccounts = source.filter(
      (account) => account.kind !== "oauth" && account.kind !== "api",
    );
    const orderedAccounts = [
      ...oauthAccounts,
      ...orderedApiAccounts,
      ...remainingApiAccounts,
      ...remainingAccounts,
    ];
    return [
      ...orderedAccounts.filter((account) => account.kind === "api" && account.favorite === true),
      ...orderedAccounts.filter((account) => account.kind !== "api" || account.favorite !== true),
    ];
  }

  function accountKey(account) { return `${account.kind}:${account.id}`; }

  function accountTestResult(account) {
    if (typeof state === "undefined") return null;
    const resultMap = account.kind === "oauth"
      ? state.authPresetTestResults
      : account.kind === "claude"
        ? state.claudePresetTestResults
        : state.apiPresetTestResults;
    return resultMap instanceof Map ? resultMap.get(account.id) || null : null;
  }

  async function testAccount(account) {
    const key = accountKey(account);
    if (testingAccountKeys.has(key)) return;
    testingAccountKeys.add(key);
    renderAccounts(allAccounts);
    showStatus(`正在测试 ${account.name || "账号"}…`, "info");
    try {
      if (account.kind === "oauth") {
        await testAuthPreset(account.id, account.name);
      } else if (account.kind === "claude") {
        await testClaudePreset(account.id, account.name);
      } else {
        await testApiPreset(account.id, account.name, document.getElementById("codex-account-status"));
      }
    } catch (error) {
      showStatus(error.message || "测试失败。", "warn");
    } finally {
      testingAccountKeys.delete(key);
      renderAccounts(allAccounts);
      const result = accountTestResult(account);
      if (result) {
        showStatus(
          `${account.name || "账号"}测试${result.ok ? "通过" : "失败"}。`,
          result.ok ? "ok" : "warn",
        );
      }
    }
  }

  function reconcileSelection(accounts) {
    const valid = new Set(accounts.map(accountKey));
    for (const key of selectedAccountIds) if (!valid.has(key)) selectedAccountIds.delete(key);
  }

  function updateSelectionControls(visibleAccounts) {
    const checkbox = document.getElementById("codex-account-select-all");
    const exportButton = document.getElementById("codex-account-export");
    const usageButton = document.getElementById("codex-account-query-usage");
    const selectedVisible = visibleAccounts.filter((account) => selectedAccountIds.has(accountKey(account))).length;
    if (checkbox) {
      checkbox.checked = visibleAccounts.length > 0 && selectedVisible === visibleAccounts.length;
      checkbox.indeterminate = selectedVisible > 0 && selectedVisible < visibleAccounts.length;
    }
    if (exportButton) exportButton.disabled = selectedAccountIds.size === 0;
    const exportFileButton = document.getElementById("codex-account-export-file");
    if (exportFileButton) exportFileButton.disabled = selectedAccountIds.size === 0;
    if (usageButton) usageButton.disabled = selectedAccountIds.size === 0 || bulkUsageQueryRunning;

    // Enable/disable action buttons that require a checkbox selection
    const hasSelection = selectedAccountIds.size > 0;
    const actionButtonIds = [
      "codex-account-save-current",
      "codex-account-refresh-current",
      "codex-account-logout-current",
      "codex-account-detect-current",
      "codex-oauth-subtab-save-current",
      "codex-oauth-subtab-refresh-current",
      "codex-oauth-subtab-detect-current",
      "codex-api-subtab-detect-current",
      "claude-subtab-detect-current",
      "codebuddy-detect-current",
      "devin-detect-current",
      "auth-detail-save-current-btn",
      "auth-detail-refresh-current-btn",
      "auth-detail-detect-current-btn",
    ];
    for (const id of actionButtonIds) {
      const btn = document.getElementById(id);
      if (btn) btn.disabled = !hasSelection;
    }
  }

  async function writeClipboard(text) {
    try {
      if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); return true; }
    } catch { /* use the synchronous browser fallback */ }
    const area = document.createElement("textarea"); area.value = text;
    area.style.cssText = "position:fixed;left:-9999px;top:0"; document.body.appendChild(area);
    area.focus(); area.select();
    try { return Boolean(document.execCommand?.("copy")); } finally { area.remove(); }
  }

  function downloadJsonFile(filename, textOrObj) {
    const text = typeof textOrObj === "string" ? textOrObj : `${JSON.stringify(textOrObj, null, 2)}\n`;
    const blob = new Blob([text], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  async function generateAccountsExportBundle(selected) {
    const sectionMap = { oauth: "auth_presets", api: "api_presets", claude: "claude_presets", devin: "devin_presets", codebuddy: "codebuddy_presets" };
    const sections = {};
    for (const kind of ACCOUNT_KINDS) {
      const section = sectionMap[kind];
      if (!section) continue;
      const ids = selected.filter((account) => account.kind === kind).map((account) => account.id);
      if (!ids.length) continue;
      sections[section] = await requestJson(`/api/settings/preset-config/clipboard/${section}/export`, {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ids }),
      });
    }
    return { format: "webclx-account-presets-bundle", version: 1, sections };
  }

  async function exportSelectedAccounts() {
    const selected = allAccounts.filter((account) => selectedAccountIds.has(accountKey(account)));
    if (!selected.length) { showStatus("请先勾选要导出的账号。", "warn"); return; }
    try {
      showStatus(`正在导出 ${selected.length} 个账号…`, "info");
      const bundle = await generateAccountsExportBundle(selected);
      if (!await writeClipboard(`${JSON.stringify(bundle, null, 2)}\n`)) throw new Error("无法写入剪贴板，请检查浏览器权限。");
      showStatus(`已导出 ${selected.length} 个账号到剪贴板。`, "ok");
    } catch (error) { showStatus(error.message, "warn"); }
  }

  async function exportSelectedAccountsToFile() {
    const selected = allAccounts.filter((account) => selectedAccountIds.has(accountKey(account)));
    if (!selected.length) { showStatus("请先勾选要导出的账号。", "warn"); return; }
    try {
      showStatus(`正在导出 ${selected.length} 个账号…`, "info");
      const bundle = await generateAccountsExportBundle(selected);
      const filename = `webclx-accounts-${new Date().toISOString().slice(0, 10)}.json`;
      downloadJsonFile(filename, bundle);
      showStatus(`已导出 ${selected.length} 个账号到文件 ${filename}。`, "ok");
    } catch (error) { showStatus(error.message, "warn"); }
  }

  async function applyImportAccountsPayload(payload, sourceLabel = "剪贴板") {
    const sections = payload?.format === "webclx-account-presets-bundle" ? payload.sections : { [payload?.section]: payload };
    const validSections = new Set(["auth_presets", "api_presets", "claude_presets", "devin_presets", "devin_accounts", "codebuddy_presets", "codebuddy_accounts"]);
    const entries = Object.entries(sections || {}).filter(([section, value]) => validSections.has(section) && Array.isArray(value?.accounts));
    if (!entries.length) {
      if (Array.isArray(payload) && typeof WebClxCodeBuddyManager?.importAccountsData === "function") {
        await WebClxCodeBuddyManager.importAccountsData(payload);
        return;
      }
      throw new Error(`没有可导入的有效账号数据。`);
    }
    const count = entries.reduce((total, [, value]) => total + value.accounts.length, 0);
    if (!window.confirm(`确定从${sourceLabel}导入 ${count} 个账号吗？`)) return;
    for (const [section, value] of entries) await requestJson(`/api/settings/preset-config/clipboard/${section}/import`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(value),
    });
    await (typeof refreshAuthPanels === "function" ? refreshAuthPanels() : loadCodexAccounts());
    showStatus(`已从${sourceLabel}导入 ${count} 个账号。`, "ok");
  }

  async function importAccountsFromClipboard() {
    try {
      let rawText = "";
      if (navigator.clipboard?.readText) {
        try { rawText = await navigator.clipboard.readText(); } catch { /* use prompt fallback */ }
      }
      if (!rawText && window.prompt) rawText = window.prompt("粘贴账号列表 JSON") || "";
      if (!rawText) throw new Error("无法读取剪贴板，请手动粘贴账号列表 JSON。");
      const payload = JSON.parse(rawText);
      await applyImportAccountsPayload(payload, "剪贴板");
    } catch (error) { showStatus(error.message || "导入账号列表失败。", "warn"); }
  }

  function triggerImportAccountsFile() {
    const input = document.getElementById("codex-account-import-file");
    if (input) {
      input.value = "";
      input.click();
    }
  }

  async function handleImportAccountsFile(event) {
    const file = event?.target?.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const payload = JSON.parse(text);
      await applyImportAccountsPayload(payload, "文件");
    } catch (error) {
      showStatus(`读取文件失败：${error.message || error}`, "warn");
    }
  }

  function oauthEditor() {
    return document.querySelector("#api-view > .auth-layout-merged");
  }

  function showOauthEditor(presetId = "") {
    setAccountKind("oauth");
    const editor = oauthEditor();
    if (!editor) return;
    editor.hidden = false;
    if (presetId && typeof editAuthPreset === "function") {
      editAuthPreset(presetId);
    } else if (typeof resetAuthPresetForm === "function") {
      resetAuthPresetForm("填写 OAuth 账号信息后新增。", "muted");
    }
    editor.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function openManager(kind, presetId = "") {
    if (kind === "oauth") {
      showOauthEditor(presetId);
    } else if (kind === "claude") {
      if (typeof setActiveTab === "function") setActiveTab("api");
      if (presetId && typeof editClaudePreset === "function") {
        editClaudePreset(presetId);
        document.getElementById("claude-view")?.scrollIntoView({ behavior: "smooth", block: "start" });
      } else {
        document.getElementById("claude-add-preset")?.click();
      }
    } else if (presetId && typeof editApiPreset === "function") {
      if (typeof setActiveTab === "function") setActiveTab("api");
      setAccountKind("api");
      editApiPreset(presetId);
    } else {
      if (typeof setActiveTab === "function") setActiveTab("api");
      if (typeof startNewApiPreset === "function") startNewApiPreset();
      setAccountKind("api");
    }
  }

  function installOauthEditor() {
    const authView = document.getElementById("auth-view");
    const apiView = document.getElementById("api-view");
    const accountList = apiView?.querySelector(".codex-account-list-card");
    const editor = apiView?.querySelector(".auth-layout-merged") ||
      authView?.querySelector(".auth-layout-merged");
    if (!apiView || !accountList || !editor) return;
    editor.hidden = true;
    editor.classList.add("codex-oauth-editor");
    accountList.insertAdjacentElement("afterend", editor);

    const panelHead = editor.querySelector(".panel-head.wide");
    if (panelHead && !panelHead.querySelector(".codex-oauth-editor-close")) {
      const close = actionButton("关闭", () => { editor.hidden = true; });
      close.classList.add("codex-oauth-editor-close");
      close.textContent = "×";
      close.title = "关闭 OAuth 编辑表单";
      close.setAttribute("aria-label", "关闭 OAuth 编辑表单");
      panelHead.appendChild(close);
    }
  }

  // ---- 用量窗口（学习 sub2api 账号表格）----
  // OAuth 行展示已采样的 5h/7d 窗口（单击触发 wham/usage 主动刷新）；
  // API 行对可识别平台（智谱 GLM / MiniMax / OpenRouter）自动查询套餐
  // 用量，与 sub2api 挂载时加载用量窗口的行为对齐。其余类型展示 "-"。

  function detectUsagePlatform(baseUrl) {
    const url = String(baseUrl || "").toLowerCase();
    if (/minimaxi\.com|minimax\.io|\/api\/codex-proxy\/minimax\//.test(url)) return "MINIMAX";
    if (/openrouter\.ai/.test(url)) return "OPENROUTER";
    if (/bigmodel\.cn|zcode\.z\.ai|\/api\/codex-proxy\/zhipu\//.test(url)) return "ZHIPU";
    return "";
  }

  function formatUsageResetMs(ts) {
    const d = new Date(Number(ts));
    if (Number.isNaN(d.getTime())) return "";
    const pad = (n) => String(n).padStart(2, "0");
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  function usageWindowLine(label, text) {
    const line = document.createElement("div");
    line.className = "usage-window-line";
    line.textContent = `${label} ${text}`;
    return line;
  }

  function usageToneClass(pct) {
    if (!Number.isFinite(pct)) return "";
    return pct >= 90 ? "sub2api-usage-danger" : pct >= 70 ? "sub2api-usage-warn" : "sub2api-usage-ok";
  }

  // sub2api UsageProgressBar 同款倒计时：到期且用量>0 记「待刷新」，
  // 用量为 0 或已到期记「现在」，其余显示 Nd Nh / Nh Nm / Nm。
  function formatUsageCountdown(resetSeconds, utilization) {
    const hasReset = Number.isFinite(resetSeconds);
    if (!hasReset) {
      return Number.isFinite(utilization) && utilization <= 0 ? "现在" : "-";
    }
    const diffMs = resetSeconds * 1000 - Date.now();
    if (diffMs <= 0) {
      return Number.isFinite(utilization) && utilization > 0 ? "待刷新" : "现在";
    }
    const totalMinutes = Math.ceil(diffMs / 60000);
    const days = Math.floor(totalMinutes / 1440);
    const hours = Math.floor((totalMinutes % 1440) / 60);
    const minutes = totalMinutes % 60;
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  }

  // [label 徽标][迷你进度条][百分比][倒计时]，与 sub2api UsageProgressBar 一致。
  // 徽标+进度条+百分比+倒计时包进 .usage-bar-body，悬停下划线只作用于它，
  // 行尾操作按钮保持独立可点。
  function usageWindowBar(label, percentage, resetSeconds) {
    const line = document.createElement("div");
    line.className = "usage-window-bar";
    const body = document.createElement("span");
    body.className = "usage-bar-body";
    const badge = document.createElement("span");
    badge.className = "usage-window-label";
    badge.textContent = label;
    const bar = document.createElement("span");
    bar.className = "usage-mini-bar";
    const fill = document.createElement("span");
    fill.className = "usage-mini-bar-fill";
    const pct = Number(percentage);
    fill.style.width = `${Number.isFinite(pct) ? Math.min(Math.max(pct, 0), 100) : 0}%`;
    bar.appendChild(fill);
    const pctEl = document.createElement("span");
    pctEl.className = "usage-window-pct";
    pctEl.textContent = Number.isFinite(pct) ? `${Math.round(pct)}%` : "-";
    const tone = usageToneClass(pct);
    if (tone) pctEl.classList.add(tone);
    const resetEl = document.createElement("span");
    resetEl.className = "usage-window-reset";
    resetEl.textContent = formatUsageCountdown(resetSeconds, pct);
    body.append(badge, bar, pctEl, resetEl);
    line.appendChild(body);
    return line;
  }

  function usageActionButton(label, handler, { danger = false, title = "" } = {}) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "usage-mini-action";
    if (danger) button.classList.add("danger");
    if (title) button.title = title;
    button.textContent = label;
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      void handler();
    });
    return button;
  }

  // Codex OAuth：preset.details 里的 hourly/weekly 即 wham/usage 采样，
  // 相当于 sub2api 的被动数据；操作按钮与 sub2api OpenAI 账号一致：
  // 查询(=主动刷新窗口+次数+点数) / 次数N / 重置 / 点数 / 邀请用户，
  // 并入 5h/7d 两行末尾，单元格保持两行。
  function renderOauthUsageWindow(cell, account) {
    const quota = account.quota || {};
    cell.replaceChildren();
    cell.classList.remove("sub2api-usage-ok", "sub2api-usage-warn", "sub2api-usage-danger");
    const resetCount = Number.isFinite(quota.reset_credit_count) ? quota.reset_credit_count : null;
    const hourlyActions = document.createElement("span");
    hourlyActions.className = "usage-bar-actions";
    hourlyActions.append(
      usageActionButton("查询", () => refreshOauthUsageWindow(account), { title: "主动刷新用量窗口与点数" }),
      Object.assign(document.createElement("span"), {
        className: "usage-mini-stat",
        textContent: `次数 ${resetCount === null ? "—" : resetCount}`,
      }),
      usageActionButton("重置", () => consumeOauthResetCredit(account), {
        title: resetCount ? "消耗一个额度重置点数，立即重置用量窗口" : "没有可用的重置点数，先点「查询」",
      }),
    );
    const weeklyActions = document.createElement("span");
    weeklyActions.className = "usage-bar-actions";
    weeklyActions.append(
      Object.assign(document.createElement("span"), {
        className: "usage-mini-stat",
        textContent: `点数 ${quota.codex_points || "—"}`,
      }),
      usageActionButton("邀请", () => inviteOauthReferral(account), { title: "查询可邀请名额并发送 Codex 邀请" }),
    );
    const has = Number.isFinite(quota.hourly_percentage) || Number.isFinite(quota.weekly_percentage);
    if (has) {
      const hourlyBar = usageWindowBar("5h", quota.hourly_percentage, quota.hourly_reset_time);
      const weeklyBar = usageWindowBar("7d", quota.weekly_percentage, quota.weekly_reset_time);
      hourlyBar.appendChild(hourlyActions);
      weeklyBar.appendChild(weeklyActions);
      cell.append(hourlyBar, weeklyBar);
      const maxPct = Math.max(quota.hourly_percentage ?? 0, quota.weekly_percentage ?? 0);
      const tone = usageToneClass(maxPct);
      if (tone) cell.classList.add(tone);
      return;
    }

    const refreshError =
      typeof state !== "undefined" ? state.authRefreshErrorsByPresetId?.get(account.id) : null;
    const emptyLine = document.createElement("div");
    emptyLine.className = "usage-window-line";
    if (refreshError) {
      emptyLine.textContent = "刷新失败";
      emptyLine.classList.add("sub2api-usage-danger");
      cell.title = `${refreshError}\n点「查询」重试`;
    } else {
      emptyLine.textContent = "未查询";
    }
    // 无采样数据时状态行 + 操作行，同样只占两行。
    const actions = document.createElement("div");
    actions.className = "usage-action-row";
    actions.append(hourlyActions, weeklyActions);
    cell.append(emptyLine, actions);
  }

  async function refreshOauthUsageWindow(account) {
    showStatus(`正在刷新 ${account.name || "账号"} 的用量窗口…`, "info");
    try {
      const result = await refreshAuthPresetQuota(account.id, account.name, document.getElementById("codex-account-status"));
      if (result && result.ok === false) {
        showStatus(`${account.name || "账号"} 用量刷新失败：${result.error || "未知错误"}`, "warn");
        if (typeof showToast === "function") {
          showToast(`${account.name || "账号"} 用量刷新失败：${result.error || "未知错误"}`, "warn", 8000);
        }
      }
    } finally {
      await loadCodexAccounts("oauth");
    }
  }

  async function consumeOauthResetCredit(account) {
    const quota = account.quota || {};
    const name = account.name || "账号";
    if (!Number.isFinite(quota.reset_credit_count) || quota.reset_credit_count <= 0) {
      showStatus(`${name} 没有可用的重置点数，请先点「查询」刷新。`, "warn");
      return;
    }
    if (!window.confirm(`确定消耗 ${name} 的一个额度重置点数吗？这会立即重置用量窗口。`)) return;
    showStatus(`正在为 ${name} 兑换重置点数…`, "info");
    try {
      const response = await requestJson(
        `/api/auth/presets/${encodeURIComponent(account.id)}/codex-reset-credit`,
        { method: "POST" },
      );
      const message = `已为 ${name} 重置 ${response?.windows_reset ?? 0} 个用量窗口。`;
      showStatus(message, "ok");
      if (typeof showToast === "function") showToast(message, "ok", 5000);
    } catch (error) {
      showStatus(`${name} 重置失败：${error.message}`, "warn");
      if (typeof showToast === "function") {
        showToast(`${name} 重置失败：${error.message}`, "warn", 8000);
      }
    } finally {
      await loadCodexAccounts("oauth");
    }
  }

  async function inviteOauthReferral(account) {
    const name = account.name || "账号";
    showStatus(`正在查询 ${name} 的邀请名额…`, "info");
    try {
      const response = await requestJson(
        `/api/auth/presets/${encodeURIComponent(account.id)}/codex-referral`,
      );
      const info = response?.referral || {};
      const remaining = Number.isFinite(info.available_invites) ? info.available_invites : null;
      const capacityText = info.ineligible_reason
        ? String(info.ineligible_reason)
        : remaining === null
          ? "邀请名额未知"
          : `剩余可邀请 ${remaining} 人`;
      if (info.ineligible_reason || remaining === 0) {
        showStatus(`${name} 暂不可邀请：${capacityText}`, "warn");
        if (typeof showToast === "function") {
          showToast(`${name} 暂不可邀请：${capacityText}`, "warn", 6000);
        }
        return;
      }
      const email = window.prompt(
        `${name} ${capacityText}。${info.requires_confirmation ? "发送即表示确认邀请对方。" : ""}\n请输入要邀请的邮箱：`,
      );
      if (email === null) return;
      if (!email.trim() || !email.includes("@")) {
        showStatus("请输入有效的邮箱地址。", "warn");
        return;
      }
      showStatus(`正在通过 ${name} 邀请 ${email.trim()}…`, "info");
      await requestJson(`/api/auth/presets/${encodeURIComponent(account.id)}/codex-referral`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email.trim() }),
      });
      const message = `已向 ${email.trim()} 发出 Codex 邀请。`;
      showStatus(message, "ok");
      if (typeof showToast === "function") showToast(message, "ok", 5000);
    } catch (error) {
      showStatus(`${name} 邀请失败：${error.message}`, "warn");
      if (typeof showToast === "function") {
        showToast(`${name} 邀请失败：${error.message}`, "warn", 8000);
      }
    }
  }

  // API 预设：按 base_url 识别平台后调用 /api/quota/query（智谱/MiniMax）
  // 或 OpenRouter /api/v1/key（前端直连）。识别不出的 base_url 显示 "-"。
  let apiUsageInFlight = 0;
  const apiUsageQueue = [];
  function enqueueApiUsage(task) {
    return new Promise((resolve) => {
      apiUsageQueue.push(async () => {
        try {
          resolve(await task());
        } finally {
          apiUsageInFlight -= 1;
          drainApiUsageQueue();
        }
      });
      drainApiUsageQueue();
    });
  }
  function drainApiUsageQueue() {
    while (apiUsageInFlight < 4 && apiUsageQueue.length) {
      apiUsageInFlight += 1;
      apiUsageQueue.shift()();
    }
  }

  async function fetchApiUsage(account) {
    const platform = detectUsagePlatform(account.quota_base_url || account.endpoint);
    if (platform === "OPENROUTER") {
      const response = await fetch("https://openrouter.ai/api/v1/key", {
        headers: { Authorization: `Bearer ${account.api_key}` },
      });
      if (!response.ok) throw new Error(`OpenRouter HTTP ${response.status}`);
      const data = await response.json();
      return { platform, raw: data?.data || {} };
    }
    const params = new URLSearchParams({
      base_url: account.quota_base_url || account.endpoint || "",
      api_key: account.api_key || "",
    });
    const data = await requestJson(`/api/quota/query?${params.toString()}`);
    return { platform, raw: data };
  }

  // API 用量查询结果按账号缓存：整表重渲（勾选、切换等）时直接回放
  // 缓存，只有初始加载与显式「查询」才真的打上游。
  const apiUsageResults = new Map();

  async function queryApiUsage(account) {
    const key = accountKey(account);
    try {
      const result = await enqueueApiUsage(() => fetchApiUsage(account));
      apiUsageResults.set(key, { ok: true, result });
      return result;
    } catch (error) {
      apiUsageResults.set(key, { ok: false, message: error?.message || String(error) });
      throw error;
    }
  }

  function pickMinimaxModelRemains(remains, model) {
    const list = Array.isArray(remains?.model_remains) ? remains.model_remains : [];
    if (!list.length) return null;
    const wanted = String(model || "").toLowerCase();
    return list.find((m) => wanted && String(m.model_name || "").toLowerCase() === wanted)
      || list.find((m) => wanted && String(m.model_name || "").toLowerCase().includes(wanted))
      || list[0];
  }

  function renderApiUsageData(cell, account, result) {
    cell.replaceChildren();
    cell.classList.remove("sub2api-usage-ok", "sub2api-usage-warn", "sub2api-usage-danger");
    const titles = [];
    if (result.platform === "ZHIPU") {
      const limits = result.raw?.quota_limit?.limits || [];
      const order = { TOKENS_LIMIT: "Token", TIME_LIMIT: "MCP" };
      const rows = limits.filter((l) => order[l.type]);
      if (!rows.length) {
        cell.textContent = "无额度数据";
        return;
      }
      let maxPct = 0;
      for (const limit of rows) {
        const pct = Number(limit.percentage);
        if (Number.isFinite(pct)) maxPct = Math.max(maxPct, pct);
        cell.appendChild(usageWindowLine(order[limit.type], `${Number.isFinite(pct) ? pct : "-"}%`));
        const reset = formatUsageResetMs(limit.nextResetTime);
        titles.push(`${order[limit.type]} 已用 ${limit.percentage ?? "-"}%${reset ? `，重置 ${reset}` : ""}`);
      }
      const tone = usageToneClass(maxPct);
      if (tone) cell.classList.add(tone);
    } else if (result.platform === "MINIMAX") {
      const modelRemains = pickMinimaxModelRemains(result.raw?.remains, account.model);
      if (!modelRemains) {
        cell.textContent = "无额度数据";
        return;
      }
      const fiveUsed = 100 - Number(modelRemains.current_interval_remaining_percent ?? 100);
      const weekUsed = 100 - Number(modelRemains.current_weekly_remaining_percent ?? 100);
      cell.append(
        usageWindowLine("5h", `${fiveUsed.toFixed(0)}%`),
        usageWindowLine("7d", `${weekUsed.toFixed(0)}%`),
      );
      const fiveReset = formatUsageResetMs(modelRemains.end_time);
      const weekReset = formatUsageResetMs(modelRemains.weekly_end_time);
      titles.push(
        `${modelRemains.model_name || "模型"} 5h 已用 ${fiveUsed.toFixed(0)}%${fiveReset ? `，重置 ${fiveReset}` : ""}`,
        `7d 已用 ${weekUsed.toFixed(0)}%${weekReset ? `，重置 ${weekReset}` : ""}`,
      );
      const tone = usageToneClass(Math.max(fiveUsed, weekUsed));
      if (tone) cell.classList.add(tone);
    } else if (result.platform === "OPENROUTER") {
      const raw = result.raw || {};
      const remaining = Number(raw.limit_remaining);
      const usage = Number(raw.usage);
      cell.textContent = Number.isFinite(remaining)
        ? `剩 $${remaining.toFixed(2)}`
        : Number.isFinite(usage)
          ? `已用 $${usage.toFixed(2)}`
          : "无额度数据";
      if (Number.isFinite(usage)) titles.push(`已用 $${usage.toFixed(2)}`);
      if (Number.isFinite(remaining)) titles.push(`剩余 $${remaining.toFixed(2)}`);
    } else {
      cell.textContent = "-";
      return;
    }
    if (titles.length) cell.title = `${titles.join("\n")}\n单击重新查询`;
  }

  function renderApiUsageWindow(cell, account) {
    const platform = detectUsagePlatform(account.quota_base_url || account.endpoint);
    if (!platform || !account.api_key) {
      cell.textContent = "-";
      return;
    }
    const cached = apiUsageResults.get(accountKey(account));
    const query = (notify = false) => {
      cell.textContent = "查询中…";
      queryApiUsage(account)
        .then((result) => renderApiUsageData(cell, account, result))
        .catch((error) => {
          cell.textContent = "查询失败";
          cell.title = `${error?.message || error}\n单击重新查询`;
          if (notify && typeof showToast === "function") {
            showToast(`${account.name || "账号"} 用量查询失败：${error?.message || error}`, "warn", 8000);
          }
        });
    };
    cell.classList.add("usage-window-clickable");
    cell.title = "单击重新查询";
    cell.addEventListener("click", () => query(true));
    if (cached) {
      if (cached.ok) {
        renderApiUsageData(cell, account, cached.result);
      } else {
        cell.textContent = "查询失败";
        cell.title = `${cached.message}\n单击重新查询`;
      }
      return;
    }
    query();
  }

  // 工具栏「查询用量」：对勾选的 OAuth 账号走 refresh-quota 主动采样，
  // 对勾选的 API 账号重跑套餐用量查询；不支持的类型跳过并计数。
  let bulkUsageQueryRunning = false;
  async function queryUsageForSelected() {
    if (bulkUsageQueryRunning) return;
    const button = document.getElementById("codex-account-query-usage");
    const selected = allAccounts.filter((account) => selectedAccountIds.has(accountKey(account)));
    const oauthAccounts = selected.filter((account) => account.kind === "oauth");
    const apiAccounts = selected.filter(
      (account) => account.kind === "api" && detectUsagePlatform(account.quota_base_url || account.endpoint),
    );
    const skipped = selected.length - oauthAccounts.length - apiAccounts.length;
    if (!oauthAccounts.length && !apiAccounts.length) {
      showStatus("选中的账号类型不支持用量窗口查询。", "warn");
      return;
    }

    bulkUsageQueryRunning = true;
    if (button) button.disabled = true;
    showStatus(
      `正在查询 ${oauthAccounts.length + apiAccounts.length} 个账号的用量窗口${skipped ? `（跳过 ${skipped} 个不支持的类型）` : ""}…`,
      "info",
    );
    try {
      const oauthResults = await Promise.allSettled(
        oauthAccounts.map(async (account) => {
          const result = await refreshAuthPresetQuota(account.id, account.name, document.getElementById("codex-account-status"));
          if (result && result.ok === false) {
            throw new Error(`${account.name || account.id}：${result.error || "刷新失败"}`);
          }
          return result;
        }),
      );
      const apiResults = await Promise.allSettled(
        apiAccounts.map((account) => queryApiUsage(account)),
      );
      const failures = [...oauthResults, ...apiResults].filter((r) => r.status === "rejected");
      const failed = failures.length;
      const succeeded = oauthResults.length + apiResults.length - failed;
      showStatus(
        `用量查询完成：成功 ${succeeded} 个${failed ? `，失败 ${failed} 个` : ""}${skipped ? `，跳过 ${skipped} 个` : ""}。`,
        failed ? "warn" : "ok",
      );
      if (failed && typeof showToast === "function") {
        const detail = failures
          .slice(0, 3)
          .map((r) => r.reason?.message || String(r.reason))
          .join("；");
        showToast(
          `用量查询失败 ${failed} 个：${detail}${failed > 3 ? ` 等共 ${failed} 个` : ""}`,
          "warn",
          8000,
        );
      }
    } finally {
      bulkUsageQueryRunning = false;
      if (button) button.disabled = false;
      await loadCodexAccounts(activeKind);
    }
  }

  function usageWindowCell(account) {
    const cell = textCell("-", "usage-window-cell mono-text");
    if (account.kind === "oauth") {
      renderOauthUsageWindow(cell, account);
      cell.classList.add("usage-window-clickable");
      if (!cell.title) cell.title = "单击刷新账号额度";
      cell.addEventListener("click", () => refreshOauthUsageWindow(account));
    } else if (account.kind === "api") {
      renderApiUsageWindow(cell, account);
    }
    return cell;
  }

  function renderAccounts(accounts) {
    const list = document.getElementById("codex-account-list");
    if (!list) return;
    list.replaceChildren();
    const visibleAccounts = filteredAccounts(accounts);
    reconcileSelection(accounts);
    updateSelectionControls(visibleAccounts);
    if (!visibleAccounts.length) {
      const row = document.createElement("tr");
      const cell = textCell("当前类型下没有账号。");
      cell.colSpan = 15;
      row.appendChild(cell);
      list.appendChild(row);
      return;
    }

    for (const account of visibleAccounts) {
      const row = document.createElement("tr");
      if (account.active) row.classList.add("current-row");
      if (selectedAccountIds.has(accountKey(account))) row.classList.add("selected-row");

      const selectionCell = document.createElement("td");
      selectionCell.className = "codex-account-selection-cell";
      const selection = document.createElement("input");
      selection.type = "checkbox";
      selection.checked = selectedAccountIds.has(accountKey(account));
      selection.setAttribute("aria-label", `选择账号 ${account.name || account.id}`);
      selection.addEventListener("change", () => {
        const key = accountKey(account);
        if (selection.checked) selectedAccountIds.add(key); else selectedAccountIds.delete(key);
        renderAccounts(allAccounts);
      });
      selectionCell.appendChild(selection);

      // 操作列固定且手机屏宽有限：行内只保留一个 ⋮ 菜单按钮，
      // 所有命令收进弹出菜单（与预设表格 createPresetActionMenu 同款交互）。
      const actionItems = [
        {
          label: "切换到此账号",
          disabled: Boolean(account.active),
          title: account.active ? "已是当前账号" : "",
          handler: () => applyAccount(account),
        },
      ];

      const status = textCell(account.active ? "当前" : "");
      status.className = account.active ? "current-indicator active" : "current-indicator";
      const testResultCell = createCurrentIndicatorCell(false, {
        testResult: accountTestResult(account),
        testKind: account.kind === "oauth" ? "auth" : account.kind,
        testing: testingAccountKeys.has(accountKey(account)),
        showUntested: true,
      });
      testResultCell.classList.add("codex-account-test-result");
      const kindLabel = account.kind === "oauth"
        ? "Codex_OAuth"
        : account.kind === "claude" ? "Claude_API"
        : account.kind === "codebuddy" ? "CodeBuddy"
        : account.kind === "devin" ? "Devin"
        : "Codex_API";
      const kind = textCell(kindLabel, "mono-text");
      if (account.kind !== "codebuddy" && account.kind !== "devin") {
        actionItems.push({ label: "编辑", handler: () => openManager(account.kind, account.id) });
      }
      if (account.kind === "codebuddy") {
        if (account.supportsCheckin !== false) {
          actionItems.push({ label: "签到", handler: () => WebClxCodeBuddyManager.checkinAccount(account) });
        }
        if (account.supportsTravel !== false) {
          actionItems.push({ label: "旅行", handler: () => WebClxCodeBuddyManager.travelAccount(account) });
        }
        actionItems.push(
          { label: "积分", handler: () => WebClxCodeBuddyManager.creditsAccount(account) },
        );
      }
      if (account.kind === "api") {
        actionItems.push({ label: "测试", handler: () => testAccount(account) });
      }
      if (account.kind === "claude") {
        actionItems.push(
          { label: "测试", handler: () => testAccount(account) },
          { label: "OpenCode", handler: () => applyClaudePresetToOpencode(account.id, document.getElementById("codex-account-status")) },
        );
      }
      if (account.kind === "oauth") {
        if (account.active) {
          actionItems.push({ label: "刷新登录", handler: () => refreshCurrentCodexLogin() });
        }
        actionItems.push(
          { label: "刷新", handler: () => refreshAuthPresetQuota(account.id, account.name, document.getElementById("codex-account-status")) },
          { label: "测试", handler: () => testAccount(account) },
        );
      }
      actionItems.push({ label: "删除", danger: true, handler: () => deleteAccount(account) });
      const actionCell = document.createElement("td");
      actionCell.className = "codex-account-action-cell";
      actionCell.appendChild(
        createPresetActionMenu(actionItems, { label: `${account.name || "账号"} 的操作` }),
      );

      const favoriteCell = document.createElement("td");
      favoriteCell.className = "api-preset-favorite-cell";
      if (account.kind === "api") {
        const favoriteButton = actionButton(
          account.favorite ? "★" : "☆",
          () => toggleApiAccountFavorite(account),
          Boolean(account.favorite),
        );
        favoriteButton.classList.add("api-preset-favorite-button");
        favoriteButton.setAttribute("aria-pressed", account.favorite ? "true" : "false");
        favoriteButton.setAttribute(
          "aria-label",
          account.favorite
            ? `取消收藏 ${account.name || account.id}`
            : `收藏 ${account.name || account.id}`,
        );
        favoriteButton.title = account.favorite ? "取消收藏" : "加入收藏";
        favoriteCell.appendChild(favoriteButton);
      } else {
        favoriteCell.textContent = "-";
      }

      row.append(
        selectionCell,
        actionCell,
        favoriteCell,
        status,
        testResultCell,
        kind,
        textCell(account.name),
        textCell(account.account, "mono-text"),
        usageWindowCell(account),
        creditsTotalCell(account),
        creditsBreakdownCell(account),
        creditsExpiryCell(account),
        textCell(account.model, "mono-text"),
        textCell(account.endpoint, "mono-text"),
        textCell(String(account.switch_count || 0), "mono-text"),
      );
      list.appendChild(row);
    }
  }

  let accountReloadPending = false;
  let accountLoadPromise = null;

  function loadCodexAccounts(kind) {
    if (kind && typeof kind === "string" && kind !== "all") {
      return loadAccountsForKind(kind);
    }
    if (!document.getElementById("codex-account-list")) return Promise.resolve();
    if (accountLoadPromise) {
      accountReloadPending = true;
      return accountLoadPromise;
    }
    accountLoadPromise = (async () => {
      do {
        accountReloadPending = false;
        await loadCodexAccountsOnce();
      } while (accountReloadPending);
    })().finally(() => { accountLoadPromise = null; });
    return accountLoadPromise;
  }

  async function loadCodexAccountsOnce() {
    try {
      const [codexResponse, apiPresetResponse, authPresetResponse] = await Promise.all([
        requestJson("/api/auth/codex-accounts"),
        requestJson("/api/auth/api-presets"),
        requestJson("/api/auth/presets"),
        typeof loadClaudePresets === "function" ? loadClaudePresets().catch(() => []) : Promise.resolve(null),
      ]);
      const claudeAccounts = Array.isArray(state?.claudePresets)
        ? state.claudePresets.map((preset) => ({
          kind: "claude",
          id: preset.id,
          name: preset.name,
          active: Boolean(preset.active),
          saved_at: preset.saved_at,
          switch_count: preset.switch_count,
          account: preset.masked_auth_token || preset.provider_name,
          model: [preset.default_sonnet_model, preset.third_party_model, preset.default_opus_model]
            .filter(Boolean)
            .join(" / "),
          endpoint: preset.base_url,
        }))
        : [];
      let codebuddyAccounts = [];
      if (typeof WebClxCodeBuddyManager === "object") {
        codebuddyAccounts = (await WebClxCodeBuddyManager.loadAccounts()).map((meta) => {
          const isAi = (meta.variant || "").toLowerCase() === "ai";
          const variantLabel = isAi ? "国际版" : "国内版";
          const endpoint = isAi ? "www.codebuddy.ai" : "www.codebuddy.cn";
          return {
            kind: "codebuddy",
            id: meta.id,
            name: meta.email || meta.nickname || meta.id,
            active: Boolean(meta.active),
            saved_at: meta.createdAt || 0,
            switch_count: meta.switchCount || 0,
            account: meta.nickname || meta.uid || "-",
            model: variantLabel,
            endpoint,
            needsRelogin: Boolean(meta.needsRelogin),
            credits: meta.credits || null,
            variant: meta.variant || "cn",
            supportsCheckin: meta.supportsCheckin !== false,
            supportsTravel: meta.supportsTravel !== false,
          };
        });
      }
      let devinAccounts = [];
      if (typeof WebClxDevinManager === "object") {
        devinAccounts = (await WebClxDevinManager.loadAccounts()).map((meta) => ({
          kind: "devin",
          id: meta.id,
          name: meta.name || meta.email || meta.id,
          active: Boolean(meta.active),
          saved_at: meta.saved_at || 0,
          switch_count: 0,
          account: meta.email || meta.user_id || (meta.token ? meta.token.slice(0, 10) + "..." : "-"),
          model: meta.plan || "Devin CLI",
          endpoint: meta.org_id || "api.devin.ai",
        }));
      }
      // 用量窗口数据源：oauth 行取 auth 预设 details 里已采样的
      // hourly/weekly（对应 sub2api 的被动窗口）；api 行需要预设的
      // api_key/base_url 才能调 /api/quota/query。
      const authDetailsById = new Map(
        (authPresetResponse?.presets || []).map((preset) => [preset.id, preset.details || {}]),
      );
      const apiPresetById = new Map(
        (apiPresetResponse?.presets || []).map((preset) => [preset.id, preset]),
      );
      const mergedCodexAccounts = orderCodexAccountsByPresetTables(codexResponse?.accounts, apiPresetResponse?.presets).map((account) => {
        if (account.kind === "oauth") {
          const details = authDetailsById.get(account.id) || {};
          return {
            ...account,
            quota: {
              hourly_percentage: details.hourly_percentage,
              hourly_reset_time: details.hourly_reset_time,
              weekly_percentage: details.weekly_percentage,
              weekly_reset_time: details.weekly_reset_time,
              reset_credit_count: details.reset_credit_count,
              codex_points: details.codex_points,
            },
          };
        }
        if (account.kind === "api") {
          const preset = apiPresetById.get(account.id);
          return {
            ...account,
            api_key: preset?.api_key || "",
            quota_base_url: preset?.base_url || account.endpoint || "",
          };
        }
        return account;
      });
      allAccounts = [
        ...mergedCodexAccounts,
        ...claudeAccounts,
        ...codebuddyAccounts,
        ...devinAccounts,
      ];
      renderAccounts(allAccounts);
      const current = authPresetResponse?.current_auth;
      showStatus(
        current
          ? `当前 Codex 登录：${current.email || current.short_id || current.account_id}`
          : "当前没有 Codex 官方登录。",
        current ? "ok" : "muted",
      );
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  async function fetchOauthAccounts() {
    const [codexResponse, authPresetResponse] = await Promise.all([
      requestJson("/api/auth/codex-accounts"),
      requestJson("/api/auth/presets"),
    ]);
    const authDetailsById = new Map(
      (authPresetResponse?.presets || []).map((preset) => [preset.id, preset.details || {}]),
    );
    const source = Array.isArray(codexResponse?.accounts) ? codexResponse.accounts : [];
    const accounts = source.filter((account) => account.kind === "oauth").map((account) => {
      const details = authDetailsById.get(account.id) || {};
      return {
        ...account,
        quota: {
          hourly_percentage: details.hourly_percentage,
          hourly_reset_time: details.hourly_reset_time,
          weekly_percentage: details.weekly_percentage,
          weekly_reset_time: details.weekly_reset_time,
          reset_credit_count: details.reset_credit_count,
          codex_points: details.codex_points,
        },
      };
    });
    return { accounts, currentAuth: authPresetResponse?.current_auth };
  }

  async function fetchApiAccounts() {
    const [codexResponse, apiPresetResponse] = await Promise.all([
      requestJson("/api/auth/codex-accounts"),
      requestJson("/api/auth/api-presets"),
    ]);
    const apiPresetById = new Map(
      (apiPresetResponse?.presets || []).map((preset) => [preset.id, preset]),
    );
    const accounts = orderCodexAccountsByPresetTables(codexResponse?.accounts, apiPresetResponse?.presets)
      .filter((account) => account.kind === "api")
      .map((account) => {
        const preset = apiPresetById.get(account.id);
        return {
          ...account,
          api_key: preset?.api_key || "",
          quota_base_url: preset?.base_url || account.endpoint || "",
        };
      });
    return { accounts };
  }

  async function fetchClaudeAccounts() {
    if (typeof loadClaudePresets === "function") {
      await loadClaudePresets().catch(() => []);
    }
    const accounts = Array.isArray(state?.claudePresets)
      ? state.claudePresets.map((preset) => ({
        kind: "claude",
        id: preset.id,
        name: preset.name,
        active: Boolean(preset.active),
        saved_at: preset.saved_at,
        switch_count: preset.switch_count,
        account: preset.masked_auth_token || preset.provider_name,
        model: [preset.default_sonnet_model, preset.third_party_model, preset.default_opus_model]
          .filter(Boolean)
          .join(" / "),
        endpoint: preset.base_url,
      }))
      : [];
    return { accounts };
  }

  async function fetchCodeBuddyAccounts() {
    let accounts = [];
    if (typeof WebClxCodeBuddyManager === "object") {
      accounts = (await WebClxCodeBuddyManager.loadAccounts()).map((meta) => {
        const isAi = (meta.variant || "").toLowerCase() === "ai";
        const variantLabel = isAi ? "国际版" : "国内版";
        const endpoint = isAi ? "www.codebuddy.ai" : "www.codebuddy.cn";
        return {
          kind: "codebuddy",
          id: meta.id,
          name: meta.email || meta.nickname || meta.id,
          active: Boolean(meta.active),
          saved_at: meta.createdAt || 0,
          switch_count: meta.switchCount || 0,
          account: meta.nickname || meta.uid || "-",
          model: variantLabel,
          endpoint,
          needsRelogin: Boolean(meta.needsRelogin),
          credits: meta.credits || null,
          variant: meta.variant || "cn",
          supportsCheckin: meta.supportsCheckin !== false,
          supportsTravel: meta.supportsTravel !== false,
        };
      });
    }
    return { accounts };
  }

  async function fetchDevinAccounts() {
    let accounts = [];
    if (typeof WebClxDevinManager === "object") {
      accounts = (await WebClxDevinManager.loadAccounts()).map((meta) => ({
        kind: "devin",
        id: meta.id,
        name: meta.name || meta.email || meta.id,
        active: Boolean(meta.active),
        saved_at: meta.saved_at || 0,
        switch_count: 0,
        account: meta.email || meta.user_id || (meta.token ? meta.token.slice(0, 10) + "..." : "-"),
        model: meta.plan || "Devin CLI",
        endpoint: meta.org_id || "api.devin.ai",
      }));
    }
    return { accounts };
  }

  function replaceAccountsOfKind(kind, newAccounts) {
    const remaining = allAccounts.filter((account) => account.kind !== kind);
    allAccounts = [...remaining, ...newAccounts];
    renderAccounts(allAccounts);
  }

  async function loadAccountsForKind(kind) {
    try {
      if (kind === "oauth") {
        const { accounts, currentAuth } = await fetchOauthAccounts();
        replaceAccountsOfKind("oauth", accounts);
        showStatus(
          currentAuth
            ? `当前 Codex 登录：${currentAuth.email || currentAuth.short_id || currentAuth.account_id}`
            : "当前没有 Codex 官方登录。",
          currentAuth ? "ok" : "muted",
        );
        return;
      }
      if (kind === "api") {
        const { accounts } = await fetchApiAccounts();
        replaceAccountsOfKind("api", accounts);
        return;
      }
      if (kind === "claude") {
        const { accounts } = await fetchClaudeAccounts();
        replaceAccountsOfKind("claude", accounts);
        return;
      }
      if (kind === "codebuddy") {
        const { accounts } = await fetchCodeBuddyAccounts();
        replaceAccountsOfKind("codebuddy", accounts);
        if (typeof WebClxCodeBuddyManager?.refreshAutoBar === "function") {
          await WebClxCodeBuddyManager.refreshAutoBar();
        }
        return;
      }
      if (kind === "devin") {
        const { accounts } = await fetchDevinAccounts();
        replaceAccountsOfKind("devin", accounts);
        return;
      }
      if (kind === "agy") {
        if (typeof window.agyManager?.load === "function") {
          await window.agyManager.load();
        }
        return;
      }
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  function updateRefreshButtonTitle() {
    const btn = document.getElementById("codex-account-refresh");
    if (!btn) return;
    const labels = {
      oauth: "刷新 Codex OAuth 账号与额度",
      api: "刷新 Codex API 预设与用量",
      claude: "刷新 Claude 预设",
      codebuddy: "刷新 CodeBuddy 账号与积分",
      devin: "刷新 Devin 账号",
      agy: "刷新 AGY 账号与额度",
      opencode: "刷新 OpenCode 配置",
    };
    const title = labels[activeKind] || "刷新当前账号列表与额度";
    btn.title = title;
    btn.setAttribute("aria-label", title);
  }

  async function refreshCurrentSubtabAccounts() {
    const btn = document.getElementById("codex-account-refresh");
    if (btn) {
      btn.disabled = true;
      btn.classList.add("loading");
    }
    try {
      if (activeKind === "codebuddy") {
        showStatus("正在刷新 CodeBuddy 账号与积分…", "info");
        if (typeof WebClxCodeBuddyManager?.runAllCredits === "function") {
          await WebClxCodeBuddyManager.runAllCredits();
        } else {
          await loadAccountsForKind("codebuddy");
        }
        showStatus("已刷新 CodeBuddy 账号与积分。", "ok");
        return;
      }
      if (activeKind === "oauth") {
        showStatus("正在刷新 Codex OAuth 账号与额度…", "info");
        try {
          await requestJson("/api/auth/presets/refresh-all-quotas", { method: "PUT" });
        } catch (_) {}
        await loadAccountsForKind("oauth");
        showStatus("已刷新 Codex OAuth 账号与额度。", "ok");
        return;
      }
      if (activeKind === "api") {
        showStatus("正在刷新 Codex API 预设与用量…", "info");
        const { accounts } = await fetchApiAccounts();
        replaceAccountsOfKind("api", accounts);
        const identifiable = accounts.filter(
          (acc) => detectUsagePlatform(acc.quota_base_url || acc.endpoint),
        );
        if (identifiable.length > 0) {
          await Promise.allSettled(identifiable.map((acc) => queryApiUsage(acc)));
          const refreshed = await fetchApiAccounts();
          replaceAccountsOfKind("api", refreshed.accounts);
        }
        showStatus("已刷新 Codex API 预设与用量。", "ok");
        return;
      }
      if (activeKind === "claude") {
        showStatus("正在刷新 Claude 预设…", "info");
        await loadAccountsForKind("claude");
        showStatus("已刷新 Claude 预设。", "ok");
        return;
      }
      if (activeKind === "devin") {
        showStatus("正在刷新 Devin 账号…", "info");
        await loadAccountsForKind("devin");
        showStatus("已刷新 Devin 账号。", "ok");
        return;
      }
      if (activeKind === "agy") {
        showStatus("正在刷新 AGY 账号与额度…", "info");
        if (typeof window.agyManager?.load === "function") {
          await window.agyManager.load();
        }
        const quotaAllBtn = document.getElementById("agy-quota-all");
        if (quotaAllBtn) {
          quotaAllBtn.click();
        }
        showStatus("已刷新 AGY 账号与额度。", "ok");
        return;
      }
      if (activeKind === "opencode") {
        showStatus("正在刷新 OpenCode 配置…", "info");
        await syncOpenCodeOptionsBarVisibility();
        showStatus("已刷新 OpenCode 配置。", "ok");
        return;
      }
      await loadCodexAccountsOnce();
    } catch (error) {
      showStatus(`刷新失败：${error.message}`, "warn");
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.classList.remove("loading");
      }
    }
  }

  window.loadCodexAccounts = loadCodexAccounts;
  window.loadAccountsForKind = loadAccountsForKind;
  window.refreshCurrentSubtabAccounts = refreshCurrentSubtabAccounts;
  window.getSelectedApiAccountIds = () => Array.from(selectedAccountIds)
    .filter((key) => key.startsWith("api:"))
    .map((key) => key.slice("api:".length));

  async function saveCurrentCodexLogin() {
    showStatus("正在检查当前已登录的账号信息…", "info");
    try {
      const authData = await requestJson("/api/auth/presets");
      const currentAuth = authData?.current_auth;
      if (!currentAuth) {
        showStatus("当前没有检测到已登录的 Codex 官方账号，无法保存。", "warn");
        return;
      }

      // 自动提取账号信息并比对是否已存在
      const presets = authData?.presets || [];
      const currentId = currentAuth.account_id || "";
      const currentEmail = currentAuth.email || "";
      const existingPreset = presets.find((p) => {
        if (currentId && (p.id === currentId || p.details?.account_id === currentId)) return true;
        if (currentEmail && (p.details?.email === currentEmail || p.account === currentEmail)) return true;
        return false;
      });

      if (existingPreset) {
        const label = existingPreset.name || currentEmail || currentId || "该账号";
        const confirmed = window.confirm(`账号“${label}”已存在，是否更新其登录信息？`);
        if (!confirmed) {
          showStatus("已取消更新当前账号。", "muted");
          return;
        }
      }

      showStatus("正在保存当前登录信息…", "info");
      const response = await requestJson("/api/auth/current/preset", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: existingPreset?.name || "" }),
      });
      const presetName = response.preset?.name || currentEmail || "账号信息";
      showStatus(existingPreset ? `已更新账号信息：${presetName}` : `已保存为账号信息：${presetName}`, "ok");
      await loadCodexAccounts();
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  async function refreshCurrentCodexLogin() {
    window.isRefreshingCurrentLogin = true;
    showOauthEditor();
    if (typeof authManager?.startAuthBrowserOauthSession === "function") {
      void authManager.startAuthBrowserOauthSession();
    } else if (typeof startAuthBrowserOauthSession === "function") {
      startAuthBrowserOauthSession();
    }
    showStatus("正在刷新当前登录：请在下方打开或复制登录链接，完成后粘贴回调 code 兑换并更新登录信息。", "info");
  }

  async function logoutCurrentCodexLogin() {
    if (!window.confirm("注销当前 Codex 官方登录？现有 Codex 会话不受影响。")) return;
    showStatus("正在注销当前 Codex 官方登录…", "info");
    try {
      await requestJson("/api/auth/current", { method: "DELETE" });
      showStatus("已注销当前 Codex 登录；config.toml 配置已保留。", "ok");
      await loadCodexAccounts();
    } catch (error) {
      showStatus(error.message, "warn");
    }
  }

  async function detectCurrentCodexOAuth() {
    showStatus("正在从本地 auth.json 重新读取当前 Codex 官方登录…", "info");
    try {
      const res = await requestJson("/api/auth/codex-accounts/detect-current", { method: "POST" });
      await loadCodexAccounts("oauth");
      if (res?.matched && res?.name) {
        showStatus(`已重新读取本地 Codex 官方登录：${res.name}（${res.email || res.account_id || res.short_id || "已生效"}）`, "ok");
      } else if (res?.email || res?.account_id) {
        showStatus(`已重新读取本地 Codex 官方登录：${res.email || res.account_id}（当前账号未保存在列表）`, "info");
      } else {
        showStatus("已重新读取本地配置：当前无有效 Codex 官方登录。", "muted");
      }
    } catch (error) {
      showStatus(`重新读取当前 Codex 账号失败：${error.message}`, "warn");
    }
  }

  async function detectCurrentCodexApi() {
    showStatus("正在从本地 config.toml 重新读取当前 Codex API 配置…", "info");
    try {
      const res = await requestJson("/api/auth/api-presets/detect-current", { method: "POST" });
      await loadCodexAccounts("api");
      if (res?.matched && res?.name) {
        showStatus(`已重新读取本地 Codex API 配置：${res.name}（${res.model || res.provider || "已生效"}）`, "ok");
      } else if (res?.provider || res?.model) {
        showStatus(`已重新读取本地 Codex 配置：provider=${res.provider || "无"}，model=${res.model || "无"}（未匹配到 API 预设）`, "info");
      } else {
        showStatus("已重新读取本地配置：当前非 API 模式或未配置 API provider。", "muted");
      }
    } catch (error) {
      showStatus(`重新读取当前 Codex API 配置失败：${error.message}`, "warn");
    }
  }

  async function detectCurrentClaude() {
    showStatus("正在从本地 settings.json 重新读取当前 Claude 配置…", "info");
    try {
      const res = await requestJson("/api/auth/claude-presets/detect-current", { method: "POST" });
      if (typeof loadClaudePresets === "function") await loadClaudePresets().catch(() => []);
      await loadCodexAccounts("claude");
      if (res?.matched && res?.name) {
        showStatus(`已重新读取本地 Claude settings.json：${res.name}（${res.model || res.base_url || "已生效"}）`, "ok");
      } else if (res?.base_url || res?.model) {
        showStatus(`已重新读取本地 Claude 配置：model=${res.model || "无"}，base_url=${res.base_url || "无"}（未匹配到 Claude 预设）`, "info");
      } else {
        showStatus("已重新读取本地 Claude settings.json：未匹配到已保存的 Claude 预设。", "muted");
      }
    } catch (error) {
      showStatus(`重新读取当前 Claude 配置失败：${error.message}`, "warn");
    }
  }

  async function detectCurrentCodeBuddy() {
    if (typeof WebClxCodeBuddyManager?.detectCurrent === "function") {
      await WebClxCodeBuddyManager.detectCurrent();
    } else {
      showStatus("CodeBuddy 管理器尚未初始化。", "warn");
    }
  }

  async function detectCurrentDevin() {
    if (typeof WebClxDevinManager?.detectCurrent === "function") {
      await WebClxDevinManager.detectCurrent();
    } else {
      showStatus("Devin 管理器尚未初始化。", "warn");
    }
  }

  async function detectCurrentAgy() {
    if (typeof window.agyManager?.detectCurrent === "function") {
      await window.agyManager.detectCurrent();
    } else {
      showStatus("AGY 管理器尚未初始化。", "warn");
    }
  }

  async function detectCurrentForActiveKind() {
    switch (activeKind) {
      case "oauth":
        return detectCurrentCodexOAuth();
      case "api":
        return detectCurrentCodexApi();
      case "claude":
        return detectCurrentClaude();
      case "codebuddy":
        return detectCurrentCodeBuddy();
      case "devin":
        return detectCurrentDevin();
      case "agy":
        return detectCurrentAgy();
    }
  }

  // agy_auth 子页签：控制主卡片内 AGY 表格与选项条显隐。
  function syncAgySubtabVisibility() {
    const apiView = document.getElementById("api-view");
    const agyView = document.getElementById("agy-view");
    const agyTableWrap = document.getElementById("agy-table-wrap");
    const agyOptionsBar = document.getElementById("agy-options-bar");
    const codexTableWrap = document.querySelector(".codex-account-desktop-wrap");
    const agyActive = activeKind === "agy";
    if (apiView) apiView.classList.toggle("subtab-agy", agyActive);
    if (agyView) {
      agyView.classList.toggle("active", agyActive);
      agyView.hidden = !agyActive;
    }
    if (agyTableWrap) agyTableWrap.hidden = !agyActive;
    if (agyOptionsBar) agyOptionsBar.hidden = !agyActive;
    if (codexTableWrap) codexTableWrap.hidden = agyActive;
    if (agyActive && typeof window.agyManager?.load === "function") {
      window.agyManager.load();
    }
    if (agyActive && typeof window.agyManager?.refreshOptions === "function") {
      window.agyManager.refreshOptions();
    }
  }
  window.syncAgySubtabVisibility = syncAgySubtabVisibility;

  // CodeBuddy 页签：自动签到 / 自动旅行开关只在选中该类型时显示。
  function syncCodebuddyAutoBarVisibility() {
    const bar = document.getElementById("codebuddy-auto-bar");
    if (!bar) return;
    const active = activeKind === "codebuddy";
    bar.hidden = !active;
    if (active && typeof WebClxCodeBuddyManager?.refreshAutoBar === "function") {
      WebClxCodeBuddyManager.refreshAutoBar();
    }
  }

  // Devin 页签：全局 Bypass 开关只在选中该类型时显示。
  function syncDevinOptionsBarVisibility() {
    const bar = document.getElementById("devin-options-bar");
    if (!bar) return;
    const active = activeKind === "devin";
    bar.hidden = !active;
    if (active && typeof WebClxDevinManager?.refreshCliOptions === "function") {
      WebClxDevinManager.refreshCliOptions();
    }
  }

  async function syncOpenCodeOptionsBarVisibility() {
    const bar = document.getElementById("opencode-options-bar");
    if (!bar) return;
    const active = activeKind === "opencode";
    bar.hidden = !active;
    if (!active) return;
    const checkbox = document.getElementById("opencode-option-all-permissions");
    const note = document.getElementById("opencode-options-note");
    try {
      const response = await requestJson("/api/auth/opencode-options");
      if (checkbox && typeof response?.allowAllPermissions === "boolean") {
        checkbox.checked = response.allowAllPermissions;
      }
      if (note) note.textContent = "已读取 opencode.json";
    } catch (error) {
      if (note) { note.textContent = error.message; note.dataset.tone = "warn"; }
    }
  }

  async function saveOpenCodeOptions() {
    const checkbox = document.getElementById("opencode-option-all-permissions");
    const note = document.getElementById("opencode-options-note");
    if (!checkbox) return;
    checkbox.disabled = true;
    try {
      await requestJson("/api/auth/opencode-options", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ allow_all_permissions: checkbox.checked }),
      });
      const msg = checkbox.checked ? "已开启所有权限，重启 OpenCode 生效" : "已开启权限询问，重启 OpenCode 生效";
      if (note) { note.textContent = msg; note.dataset.tone = "ok"; }
      showStatus(msg, "ok");
    } catch (error) {
      checkbox.checked = !checkbox.checked;
      if (note) { note.textContent = error.message; note.dataset.tone = "warn"; }
      showStatus(error.message, "warn");
    } finally {
      checkbox.disabled = false;
    }
  }

  function syncUnifiedAccountTab() {
    const activeKindTab = document.querySelector(
      ".account-type-tabs .account-type-tab.active",
    );
    if (activeKindTab) activeKindTab.click();
  }
  window.syncUnifiedAccountTab = syncUnifiedAccountTab;

  // 工具栏只显示与当前账号类型相关的按钮；无 data-account-kinds 的控件始终显示。
  function syncToolbarForKind() {
    document.querySelectorAll("#api-view [data-account-kinds]").forEach((el) => {
      const kinds = String(el.dataset.accountKinds || "").split(/\s+/).filter(Boolean);
      el.hidden = kinds.length > 0 && !kinds.includes(activeKind);
    });
  }

  function mirrorCodexDialogToBar() {
    const pairs = [
      ["codex-common-approval-never", "llm-codex-approval-never"],
      ["codex-common-sandbox-full-access", "llm-codex-sandbox-full"],
      ["codex-common-animations-disabled", "llm-codex-animations-disabled"],
      ["codex-common-alt-screen-disabled", "llm-codex-alt-screen-disabled"],
      ["codex-common-daemon-disabled", "llm-codex-daemon-disabled"],
    ];
    for (const [from, to] of pairs) {
      const source = document.getElementById(from);
      const target = document.getElementById(to);
      if (source && target) target.checked = source.checked;
    }
  }

  let codexOptionsLoading = false;
  async function syncCodexOptionsFromConfig() {
    if (codexOptionsLoading) return;
    if (!document.getElementById("llm-codex-approval-never")) return;
    codexOptionsLoading = true;
    try {
      if (typeof loadCodexCommonConfig === "function") await loadCodexCommonConfig();
      mirrorCodexDialogToBar();
    } finally {
      codexOptionsLoading = false;
    }
  }

  async function saveCodexOptionsFromBar() {
    const approvalBar = document.getElementById("llm-codex-approval-never");
    const sandboxBar = document.getElementById("llm-codex-sandbox-full");
    const animationsBar = document.getElementById("llm-codex-animations-disabled");
    const altScreenBar = document.getElementById("llm-codex-alt-screen-disabled");
    const daemonBar = document.getElementById("llm-codex-daemon-disabled");
    const approvalDialog = document.getElementById("codex-common-approval-never");
    const sandboxDialog = document.getElementById("codex-common-sandbox-full-access");
    const animationsDialog = document.getElementById("codex-common-animations-disabled");
    const altScreenDialog = document.getElementById("codex-common-alt-screen-disabled");
    const daemonDialog = document.getElementById("codex-common-daemon-disabled");
    const note = document.getElementById("llm-global-options-note");
    if (
      !approvalBar ||
      !sandboxBar ||
      !animationsBar ||
      !altScreenBar ||
      !daemonBar ||
      !approvalDialog ||
      !sandboxDialog ||
      !animationsDialog ||
      !altScreenDialog ||
      !daemonDialog
    ) return;
    approvalDialog.checked = approvalBar.checked;
    sandboxDialog.checked = sandboxBar.checked;
    animationsDialog.checked = animationsBar.checked;
    altScreenDialog.checked = altScreenBar.checked;
    daemonDialog.checked = daemonBar.checked;
    approvalBar.disabled = true;
    sandboxBar.disabled = true;
    animationsBar.disabled = true;
    altScreenBar.disabled = true;
    daemonBar.disabled = true;
    try {
      if (typeof saveCodexCommonConfig === "function") await saveCodexCommonConfig();
      mirrorCodexDialogToBar();
      const status = document.getElementById("codex-common-config-status");
      const isWarn = status?.dataset.tone === "warn";
      const msg = isWarn ? (status.textContent || "保存失败。") : "已保存到 config.toml";
      if (note) {
        note.textContent = msg;
        note.dataset.tone = isWarn ? "warn" : "ok";
      }
      showStatus(msg, isWarn ? "warn" : "ok");
    } finally {
      approvalBar.disabled = false;
      sandboxBar.disabled = false;
      animationsBar.disabled = false;
      altScreenBar.disabled = false;
      daemonBar.disabled = false;
    }
  }

  // 子页签全局选项条：oauth/api 显示 Codex 公共配置开关，claude 只保留全局设置入口。
  function syncGlobalOptionsBarVisibility() {
    const bar = document.getElementById("llm-global-options-bar");
    if (!bar) return;
    const visible = ["oauth", "api", "claude"].includes(activeKind);
    bar.hidden = !visible;
    const globalButton = document.getElementById("codex-global-settings-open");
    if (globalButton) {
      globalButton.dataset.globalTab = activeKind === "claude" ? "claude" : "codex";
    }
    if (visible && (activeKind === "oauth" || activeKind === "api")) {
      void syncCodexOptionsFromConfig();
    }
  }

  function setAccountKind(kind, { syncUrl = true, push = false } = {}) {
    if (!Object.hasOwn(ACCOUNT_PATHS, kind)) return;
    activeKind = kind;
    if (activeKind !== "codebuddy") {
      const statusEl = document.getElementById?.("codex-account-status");
      if (statusEl?.dataset.tone === "muted") {
        showStatus("");
      }
    }
    document.querySelectorAll(".account-type-tabs .account-type-tab").forEach((tab) => {
      const isActive = tab.dataset.kind === activeKind;
      tab.classList.toggle("active", isActive);
      tab.setAttribute("aria-selected", isActive ? "true" : "false");
    });
    renderAccounts(allAccounts);
    syncAgySubtabVisibility();
    syncCodebuddyAutoBarVisibility();
    syncDevinOptionsBarVisibility();
    if (typeof syncOpenCodeOptionsBarVisibility === "function") {
      void syncOpenCodeOptionsBarVisibility();
    }
    syncToolbarForKind();
    syncGlobalOptionsBarVisibility();
    if (typeof updateRefreshButtonTitle === "function") {
      updateRefreshButtonTitle();
    }
    if (syncUrl && typeof syncTabUrl === "function") syncTabUrl({ push });
  }

  window.applyAccountKindFromUrl = () => {
    const kind = accountKindFromUrl();
    if (kind) setAccountKind(kind, { syncUrl: false });
  };

  window.addEventListener("DOMContentLoaded", () => {
    installOauthEditor();
    setAccountKind(activeKind, { syncUrl: false });
    document.querySelectorAll(".account-type-tabs .account-type-tab").forEach((btn) => {
      btn.addEventListener("click", () => setAccountKind(btn.dataset.kind, { push: true }));
    });
    document.getElementById("codex-account-refresh")?.addEventListener("click", refreshCurrentSubtabAccounts);
    document.getElementById("opencode-option-all-permissions")?.addEventListener("change", saveOpenCodeOptions);
    document.getElementById("codex-account-select-all")?.addEventListener("change", (event) => {
      for (const account of filteredAccounts(allAccounts)) {
        const key = accountKey(account);
        if (event.target.checked) selectedAccountIds.add(key); else selectedAccountIds.delete(key);
      }
      renderAccounts(allAccounts);
    });
    document.getElementById("codex-account-export")?.addEventListener("click", exportSelectedAccounts);
    document.getElementById("codex-account-export-file")?.addEventListener("click", exportSelectedAccountsToFile);
    document.getElementById("codex-account-query-usage")?.addEventListener("click", queryUsageForSelected);
    document.getElementById("codex-account-import")?.addEventListener("click", importAccountsFromClipboard);
    document.getElementById("codex-account-import-file-btn")?.addEventListener("click", triggerImportAccountsFile);
    document.getElementById("codex-account-import-file")?.addEventListener("change", handleImportAccountsFile);
    document.getElementById("codex-account-sub2api")?.addEventListener("click", () => {
      if (typeof window.sub2apiManager?.open === "function") {
        window.sub2apiManager.open();
      }
    });
    document.getElementById("codex-account-add-oauth")?.addEventListener("click", () => showOauthEditor());
    document.getElementById("codex-account-login-oauth")?.addEventListener("click", () => {
      showOauthEditor();
      if (typeof authManager?.startAuthBrowserOauthSession === "function") {
        void authManager.startAuthBrowserOauthSession();
      } else if (typeof startAuthBrowserOauthSession === "function") {
        startAuthBrowserOauthSession();
      } else {
        showStatus("浏览器登录组件尚未初始化，请直接在编辑器中点击“浏览器登录”。", "warn");
      }
    });
    document.getElementById("codex-oauth-subtab-login")?.addEventListener("click", () => {
      document.getElementById("codex-account-login-oauth")?.click();
    });
    document.getElementById("auth-detail-login-btn")?.addEventListener("click", () => {
      document.getElementById("codex-account-login-oauth")?.click();
    });
    document.getElementById("codex-account-save-current")?.addEventListener("click", saveCurrentCodexLogin);
    document.getElementById("codex-account-refresh-current")?.addEventListener("click", refreshCurrentCodexLogin);
    document.getElementById("codex-oauth-subtab-save-current")?.addEventListener("click", saveCurrentCodexLogin);
    document.getElementById("auth-detail-save-current-btn")?.addEventListener("click", saveCurrentCodexLogin);
    document.getElementById("codex-oauth-subtab-refresh-current")?.addEventListener("click", refreshCurrentCodexLogin);
    document.getElementById("auth-detail-refresh-current-btn")?.addEventListener("click", refreshCurrentCodexLogin);
    document.getElementById("codex-account-logout-current")?.addEventListener("click", logoutCurrentCodexLogin);
    document.getElementById("codex-account-detect-current")?.addEventListener("click", detectCurrentForActiveKind);
    document.getElementById("codex-oauth-subtab-detect-current")?.addEventListener("click", detectCurrentCodexOAuth);
    document.getElementById("codex-api-subtab-detect-current")?.addEventListener("click", detectCurrentCodexApi);
    document.getElementById("claude-subtab-detect-current")?.addEventListener("click", detectCurrentClaude);
    document.getElementById("auth-detail-detect-current-btn")?.addEventListener("click", detectCurrentCodexOAuth);
    document.getElementById("codex-account-add-api")?.addEventListener("click", () => {
      if (typeof startNewApiPreset === "function") startNewApiPreset();
    });
    document.getElementById("codex-account-add-claude")?.addEventListener("click", () => {
      openManager("claude");
    });
    document.getElementById("codex-account-add-agy")?.addEventListener("click", () => {
      const panel = document.getElementById("agy-editor-panel");
      if (panel) {
        panel.hidden = false;
        panel.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    });
    document.getElementById("llm-codex-approval-never")?.addEventListener("change", saveCodexOptionsFromBar);
    document.getElementById("llm-codex-sandbox-full")?.addEventListener("change", saveCodexOptionsFromBar);
    document.getElementById("llm-codex-animations-disabled")?.addEventListener("change", saveCodexOptionsFromBar);
    document.getElementById("llm-codex-alt-screen-disabled")?.addEventListener("change", saveCodexOptionsFromBar);
    document.getElementById("llm-codex-daemon-disabled")?.addEventListener("change", saveCodexOptionsFromBar);
    document.getElementById("codex-common-approval-never")?.addEventListener("change", mirrorCodexDialogToBar);
    document.getElementById("codex-common-sandbox-full-access")?.addEventListener("change", mirrorCodexDialogToBar);
    document.getElementById("codex-common-animations-disabled")?.addEventListener("change", mirrorCodexDialogToBar);
    document.getElementById("codex-common-alt-screen-disabled")?.addEventListener("change", mirrorCodexDialogToBar);
    document.getElementById("codex-common-daemon-disabled")?.addEventListener("change", mirrorCodexDialogToBar);
    document.getElementById("codex-global-settings-dialog")?.addEventListener("close", mirrorCodexDialogToBar);
    syncToolbarForKind();
    syncGlobalOptionsBarVisibility();
    syncDevinOptionsBarVisibility();
    loadCodexAccounts();
  });
})();
