(() => {
  "use strict";

  let oauthPollTimer = null;

  function el(id) {
    return document.getElementById(id);
  }

  function showStatus(message, tone = "info", elementId = "codex-account-status") {
    const element = el(elementId);
    if (!element) return;
    element.hidden = !message;
    element.textContent = message;
    element.dataset.tone = tone;
  }

  function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  async function loadAccounts() {
    try {
      const response = await requestJson("/api/auth/codebuddy-accounts");
      return Array.isArray(response) ? response : [];
    } catch {
      return [];
    }
  }

  // ---------------------------------------------------------------------------
  // 添加账号（OAuth 扫码 / 手动 token）
  // ---------------------------------------------------------------------------

  function setAddStatus(message, tone = "info") {
    showStatus(message, tone, "codebuddy-add-status");
  }

  function openAddDialog() {
    const dialog = el("codebuddy-add-dialog");
    if (!dialog) return;
    stopOauthPolling();
    ["codebuddy-add-access-token", "codebuddy-add-refresh-token", "codebuddy-add-name"].forEach(
      (id) => {
        if (el(id)) el(id).value = "";
      },
    );
    if (el("codebuddy-add-variant")) el("codebuddy-add-variant").value = "cn";
    const linkRow = el("codebuddy-oauth-link-row");
    if (linkRow) linkRow.hidden = true;
    setAddStatus("", "muted");
    if (!dialog.open) dialog.showModal();
  }

  function closeAddDialog() {
    const dialog = el("codebuddy-add-dialog");
    stopOauthPolling();
    if (dialog?.open) dialog.close();
  }

  function stopOauthPolling() {
    if (oauthPollTimer) {
      clearTimeout(oauthPollTimer);
      oauthPollTimer = null;
    }
  }

  async function startOauthLogin() {
    stopOauthPolling();
    const variantSelect = el("codebuddy-add-variant");
    const variant = variantSelect?.value || "cn";
    const variantLabel = variant === "ai" ? "国际版" : "国内版";
    setAddStatus(`正在申请 ${variantLabel} 登录链接…`, "info");
    try {
      const result = await requestJson("/api/auth/codebuddy-accounts/oauth/start", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ variant }),
      });
      const link = el("codebuddy-oauth-link");
      const linkRow = el("codebuddy-oauth-link-row");
      if (link && linkRow) {
        link.href = result.verificationUri || "";
        linkRow.hidden = false;
      }
      setAddStatus(`请在浏览器打开 ${variantLabel} 链接完成登录，登录成功后自动加入账号库。`, "info");
      pollOauthLogin(result.loginId);
    } catch (error) {
      setAddStatus(error.message || "申请登录链接失败。", "warn");
    }
  }

  function pollOauthLogin(loginId) {
    if (!loginId) return;
    stopOauthPolling();
    const tick = async () => {
      try {
        const result = await requestJson("/api/auth/codebuddy-accounts/oauth/poll", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ login_id: loginId }),
        });
        if (result?.done) {
          if (result.error) {
            setAddStatus(result.error, "warn");
          } else {
            setAddStatus(`已添加账号 ${result.account?.email || result.account?.nickname || result.account?.id || ""}。`, "ok");
            closeAddDialog();
          }
          if (typeof loadCodexAccounts === "function") await loadCodexAccounts();
          return;
        }
      } catch (error) {
        setAddStatus(error.message || "轮询登录状态失败。", "warn");
        return;
      }
      oauthPollTimer = setTimeout(tick, 2000);
    };
    oauthPollTimer = setTimeout(tick, 1500);
  }

  async function manualAdd(event) {
    event?.preventDefault?.();
    const accessToken = el("codebuddy-add-access-token")?.value.trim() || "";
    const refreshToken = el("codebuddy-add-refresh-token")?.value.trim() || "";
    const name = el("codebuddy-add-name")?.value.trim() || "";
    const variantSelect = el("codebuddy-add-variant");
    const variant = variantSelect?.value || "cn";
    if (!accessToken) {
      setAddStatus("请粘贴 access_token。", "warn");
      return;
    }
    setAddStatus("正在添加账号…", "info");
    try {
      const meta = await requestJson("/api/auth/codebuddy-accounts", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ access_token: accessToken, refresh_token: refreshToken, name, variant }),
      });
      setAddStatus(`已添加账号 ${meta?.email || meta?.nickname || meta?.id || ""}。`, "ok");
      closeAddDialog();
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts();
    } catch (error) {
      setAddStatus(error.message || "添加账号失败。", "warn");
    }
  }

  // ---------------------------------------------------------------------------
  // 账号操作（切换 / 签到 / 旅行 / 积分）
  // ---------------------------------------------------------------------------

  async function switchAccount(account) {
    showStatus(`正在切换 CodeBuddy CLI 账号到 ${account.name}…`, "info");
    try {
      await requestJson(
        `/api/auth/codebuddy-accounts/${encodeURIComponent(account.id)}/switch`,
        { method: "PUT" },
      );
      showStatus(`已切换到 ${account.name}，新启动的 CodeBuddy CLI 将使用该账号。`, "ok");
      await loadCodexAccounts();
      return true;
    } catch (error) {
      showStatus(error.message, "warn");
      return false;
    }
  }

  async function checkinAccount(account) {
    showStatus(`正在为 ${account.name} 签到…`, "info");
    try {
      const result = await requestJson(
        `/api/auth/codebuddy-accounts/${encodeURIComponent(account.id)}/checkin`,
        { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({}) },
      );
      const ok = result?.result !== "error";
      const resultText = result?.result === "already"
        ? "今日已签到"
        : result?.result === "success" ? "签到成功" : `签到失败：${result?.error || "未知错误"}`;
      showStatus(`${account.name} ${resultText}`, ok ? "ok" : "warn");
      return ok;
    } catch (error) {
      showStatus(error.message, "warn");
      return false;
    }
  }

  async function travelAccount(account, options = {}) {
    const { reload = true } = options;
    showStatus(`正在处理 ${account.name} 的旅行状态…`, "info");
    try {
      const result = await requestJson(
        `/api/auth/codebuddy-accounts/${encodeURIComponent(account.id)}/travel`,
        { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({}) },
      );
      let ok = false;
      if (result?.ok) {
        ok = true;
        const message = result.claimed
          ? `已领取奖励 ${result.rewardCredit ?? ""}`
          : result.dailyDone
            ? "今日旅行已完成（达到每日上限）"
            : result.state === "traveling"
              ? `旅行中${result.locationName ? `：${result.locationName}` : ""}`
              : "旅行状态已更新";
        showStatus(`${account.name} ${message}`, "ok");
      } else {
        showStatus(`${account.name} 旅行处理失败：${result?.message || "未知错误"}`, "warn");
      }
      if (reload) await loadCodexAccounts("codebuddy");
      return ok;
    } catch (error) {
      showStatus(error.message, "warn");
      return false;
    }
  }

  async function creditsAccount(account, options = {}) {
    const { reload = true } = options;
    showStatus(`正在查询 ${account.name} 的积分…`, "info");
    try {
      const result = await requestJson(
        `/api/auth/codebuddy-accounts/${encodeURIComponent(account.id)}/credits`,
      );
      let ok = false;
      if (result?.ok) {
        ok = true;
        const remaining = Number(result.totalRemaining || 0);
        const expireText = result.soonestExpireAt
          ? `，最早 ${new Date(result.soonestExpireAt).toLocaleDateString()} 到期`
          : "";
        showStatus(
          `${account.name} 剩余积分 ${Number.isFinite(remaining) ? remaining.toFixed(2) : "0"}${expireText}`,
          "ok",
        );
      } else {
        showStatus(`${account.name} 积分查询失败：${result?.error || "未知错误"}`, "warn");
      }
      if (reload) await loadCodexAccounts("codebuddy");
      return ok;
    } catch (error) {
      showStatus(error.message, "warn");
      return false;
    }
  }

  // ---------------------------------------------------------------------------
  // 自动签到 / 自动旅行开关与批量执行
  // ---------------------------------------------------------------------------

  async function loadAutoConfig() {
    try {
      const response = await requestJson("/api/auth/codebuddy-auto");
      return {
        auto: response?.auto || {},
        lastCheckinRunMs: Number(response?.lastCheckinRunMs || 0),
        lastTravelRunMs: Number(response?.lastTravelRunMs || 0),
        lastCreditsRunMs: Number(response?.lastCreditsRunMs || 0),
        accountCount: Number(response?.accountCount || 0),
      };
    } catch {
      return null;
    }
  }

  function relativeRunText(timestamp) {
    if (!timestamp) return "尚未运行";
    const diff = Date.now() - timestamp;
    if (diff < 60_000) return "刚刚";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
    return `${Math.floor(diff / 86_400_000)} 天前`;
  }

  async function refreshAutoBar() {
    const checkinBox = el("codebuddy-auto-checkin");
    const travelBox = el("codebuddy-auto-travel");
    if (!checkinBox || !travelBox) return;
    const config = await loadAutoConfig();
    if (!config) return;
    checkinBox.checked = config.auto.checkin !== false;
    travelBox.checked = config.auto.travel !== false;
    const summaryText = `${config.accountCount} 个账号 · 签到 ${relativeRunText(config.lastCheckinRunMs)}`
      + ` · 旅行 ${relativeRunText(config.lastTravelRunMs)}`
      + ` · 积分 ${relativeRunText(config.lastCreditsRunMs)}`;
    const note = el("codebuddy-auto-note");
    if (note) {
      note.hidden = true;
      note.dataset.tone = "muted";
      note.textContent = summaryText;
    }
    const currentStatus = el("codex-account-status");
    if (!currentStatus || currentStatus.dataset.tone === "muted" || currentStatus.hidden || !currentStatus.textContent) {
      showStatus(summaryText, "muted");
    }
    await refreshCliOptions();
  }

  // ---------------------------------------------------------------------------
  // CLI 选项（真实 ~/.codebuddy/settings.json）
  // ---------------------------------------------------------------------------

  async function refreshCliOptions() {
    const bypassBox = el("codebuddy-option-bypass");
    const sandboxBox = el("codebuddy-option-sandbox");
    if (!bypassBox && !sandboxBox) return;
    try {
      const response = await requestJson("/api/auth/codebuddy-options");
      if (bypassBox) bypassBox.checked = response?.bypassPermissions === true;
      if (sandboxBox) sandboxBox.checked = response?.sandbox === true;
    } catch {
      // 读取失败时保留当前勾选状态。
    }
  }

  async function saveCliOptions() {
    const bypassBox = el("codebuddy-option-bypass");
    const sandboxBox = el("codebuddy-option-sandbox");
    if (!bypassBox && !sandboxBox) return;
    const bypassPermissions = bypassBox?.checked === true;
    const sandbox = sandboxBox?.checked === true;
    try {
      await requestJson("/api/auth/codebuddy-options", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ bypassPermissions, sandbox }),
      });
      const bypassText = bypassPermissions
        ? "已开启最高权限：CodeBuddy 新会话默认 bypassPermissions，不再询问"
        : "已关闭最高权限：CodeBuddy 新会话恢复默认权限询问";
      const sandboxText = sandbox
        ? "已开启危险也不问：CODEBUDDY_IS_SANDBOX=1（已写入 ~/.bashrc，新终端会话生效）"
        : "已关闭危险也不问：已从 ~/.bashrc 移除导出";
      showStatus(`${bypassText}；${sandboxText}。`, "ok");
    } catch (error) {
      showStatus(error.message || "保存 CodeBuddy 选项失败。", "warn");
      await refreshCliOptions();
    }
  }

  async function saveAutoConfig() {
    const checkin = el("codebuddy-auto-checkin")?.checked === true;
    const travel = el("codebuddy-auto-travel")?.checked === true;
    try {
      await requestJson("/api/auth/codebuddy-auto", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ checkin, travel }),
      });
      showStatus(`已保存自动任务：签到${checkin ? "开" : "关"} / 旅行${travel ? "开" : "关"}`, "ok");
      await refreshAutoBar();
    } catch (error) {
      showStatus(error.message || "保存自动任务失败。", "warn");
      await refreshAutoBar();
    }
  }

  // 对全部账号顺序执行同一动作；返回成功数量。
  async function runForAllAccounts(label, action) {
    const accounts = await loadAccounts();
    if (!accounts.length) {
      showStatus("还没有 CodeBuddy 账号。", "warn");
      return;
    }
    showStatus(`正在对 ${accounts.length} 个账号执行${label}…`, "info");
    let succeeded = 0;
    for (const meta of accounts) {
      const account = {
        kind: "codebuddy",
        id: meta.id,
        name: meta.email || meta.nickname || meta.id,
      };
      if (await action(account, { reload: false })) succeeded += 1;
    }
    if (typeof loadCodexAccounts === "function") await loadCodexAccounts("codebuddy");
    showStatus(
      `${label}完成：${succeeded}/${accounts.length} 个账号成功。`,
      succeeded === accounts.length ? "ok" : "warn",
    );
  }

  async function detectCurrent() {
    showStatus("正在从本地 settings.json 重新读取当前 CodeBuddy 账号…", "info");
    try {
      const res = await requestJson("/api/auth/codebuddy-accounts/detect-current", { method: "POST" });
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("codebuddy");
      if (res?.matched && (res?.name || res?.id)) {
        showStatus(`已重新读取本地 CodeBuddy 当前生效账号：${res.name || res.id}（${res.account || res.id}）`, "ok");
      } else {
        showStatus("已重新读取本地 CodeBuddy 配置：当前未检测到生效账号。", "info");
      }
    } catch (error) {
      showStatus(`重新读取当前 CodeBuddy 账号失败：${error.message}`, "warn");
    }
  }

  // ---------------------------------------------------------------------------
  // 批量导入账号（JSON 数组 / wb-switch 文件）
  // ---------------------------------------------------------------------------

  async function importAccountsData(data) {
    if (!data) return;
    const items = Array.isArray(data)
      ? data
      : (data.accounts || (data.sections?.codebuddy_presets?.accounts) || (data.sections?.codebuddy_accounts?.accounts) || [data]);
    if (!Array.isArray(items) || !items.length) {
      showStatus("没有可导入的有效账号数据。", "warn");
      return;
    }
    if (!window.confirm(`确定导入 ${items.length} 个 CodeBuddy 账号吗？`)) return;
    showStatus(`正在导入 ${items.length} 个 CodeBuddy 账号…`, "info");
    try {
      const res = await requestJson("/api/auth/codebuddy-accounts/import", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(items),
      });
      const imported = res?.imported ?? 0;
      const updated = res?.updated ?? 0;
      const total = res?.total ?? (imported + updated);
      showStatus(`已成功导入 ${total} 个 CodeBuddy 账号（新增 ${imported}，更新 ${updated}）。`, "ok");
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts();
    } catch (error) {
      showStatus(error.message || "导入 CodeBuddy 账号失败。", "warn");
    }
  }

  function triggerImportFile() {
    const input = el("codebuddy-import-file-input");
    if (input) {
      input.value = "";
      input.click();
    }
  }

  async function handleImportFile(event) {
    const file = event?.target?.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text);
      await importAccountsData(parsed);
    } catch (error) {
      showStatus(error.message || "读取导入文件失败，请检查 JSON 格式。", "warn");
    }
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

  async function copyTextToClipboard(text) {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        return true;
      }
    } catch { /* use textarea fallback */ }
    const area = document.createElement("textarea");
    area.value = text;
    area.style.cssText = "position:fixed;left:-9999px;top:0";
    document.body.appendChild(area);
    area.focus();
    area.select();
    try {
      return Boolean(document.execCommand?.("copy"));
    } finally {
      area.remove();
    }
  }

  async function exportAccountsFile() {
    const accounts = await loadAccounts();
    if (!accounts.length) {
      showStatus("没有可导出的 CodeBuddy 账号。", "warn");
      return;
    }
    const filename = `codebuddy_accounts_${new Date().toISOString().slice(0, 10)}.json`;
    downloadJsonFile(filename, accounts);
    showStatus(`已导出 ${accounts.length} 个 CodeBuddy 账号到 ${filename}。`, "ok");
  }

  async function exportAccountsClipboard() {
    const accounts = await loadAccounts();
    if (!accounts.length) {
      showStatus("没有可导出的 CodeBuddy 账号。", "warn");
      return;
    }
    const text = `${JSON.stringify(accounts, null, 2)}\n`;
    if (!await copyTextToClipboard(text)) {
      showStatus("复制到剪贴板失败，请检查浏览器权限。", "warn");
      return;
    }
    showStatus(`已复制 ${accounts.length} 个 CodeBuddy 账号到剪贴板。`, "ok");
  }

  async function importAccountsFromClipboard() {
    try {
      let rawText = "";
      if (navigator.clipboard?.readText) {
        try { rawText = await navigator.clipboard.readText(); } catch { /* use prompt fallback */ }
      }
      if (!rawText && window.prompt) rawText = window.prompt("粘贴 CodeBuddy 账号 JSON") || "";
      if (!rawText) throw new Error("无法读取剪贴板，请手动粘贴 CodeBuddy 账号 JSON。");
      const parsed = JSON.parse(rawText);
      await importAccountsData(parsed);
    } catch (error) {
      showStatus(error.message || "从剪贴板读取账号失败，请检查 JSON 格式。", "warn");
    }
  }

  window.WebClxCodeBuddyManager = Object.freeze({
    loadAccounts,
    openAddDialog,
    switchAccount,
    checkinAccount,
    travelAccount,
    creditsAccount,
    refreshAutoBar,
    detectCurrent,
    importAccountsData,
    triggerImportFile,
    importAccountsFromClipboard,
    exportAccountsFile,
    exportAccountsClipboard,
    runAllCheckin: () => runForAllAccounts("签到", checkinAccount),
    runAllTravel: () => runForAllAccounts("旅行", travelAccount),
    runAllCredits: () => runForAllAccounts("积分刷新", creditsAccount),
  });

  window.addEventListener("DOMContentLoaded", () => {
    el("codex-account-add-codebuddy")?.addEventListener("click", openAddDialog);
    el("codebuddy-detect-current")?.addEventListener("click", detectCurrent);
    el("codebuddy-import-btn")?.addEventListener("click", triggerImportFile);
    el("codebuddy-import-toolbar")?.addEventListener("click", triggerImportFile);
    el("codebuddy-import-clipboard")?.addEventListener("click", importAccountsFromClipboard);
    el("codebuddy-import-file-input")?.addEventListener("change", handleImportFile);
    el("codebuddy-export-file")?.addEventListener("click", exportAccountsFile);
    el("codebuddy-export-clipboard")?.addEventListener("click", exportAccountsClipboard);
    el("codebuddy-auto-checkin")?.addEventListener("change", saveAutoConfig);
    el("codebuddy-auto-travel")?.addEventListener("change", saveAutoConfig);
    el("codebuddy-option-bypass")?.addEventListener("change", saveCliOptions);
    el("codebuddy-option-sandbox")?.addEventListener("change", saveCliOptions);
    el("codebuddy-run-checkin")?.addEventListener("click", () => {
      WebClxCodeBuddyManager.runAllCheckin();
    });
    el("codebuddy-run-travel")?.addEventListener("click", () => {
      WebClxCodeBuddyManager.runAllTravel();
    });
    el("codebuddy-refresh-credits")?.addEventListener("click", () => {
      WebClxCodeBuddyManager.runAllCredits();
    });
    el("codebuddy-add-close")?.addEventListener("click", closeAddDialog);
    el("codebuddy-oauth-start")?.addEventListener("click", startOauthLogin);
    el("codebuddy-add-form")?.addEventListener("submit", manualAdd);
    el("codebuddy-add-dialog")?.addEventListener("cancel", (event) => {
      event.preventDefault();
      closeAddDialog();
    });
  });
})();
