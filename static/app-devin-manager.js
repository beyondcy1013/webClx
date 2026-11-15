(() => {
  "use strict";

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

  async function loadAccounts(strict = false) {
    try {
      const response = await requestJson("/api/auth/devin-accounts");
      return Array.isArray(response) ? response : [];
    } catch (error) {
      if (strict) throw error;
      return [];
    }
  }

  const quotaCache = new Map();
  const quotaRequests = new Map();

  function formatDevinDateTime(timestamp) {
    if (!timestamp) return "—";
    const date = new Date(typeof timestamp === "number" ? (timestamp < 1e11 ? timestamp * 1000 : timestamp) : timestamp);
    if (Number.isNaN(date.getTime())) return "—";
    const pad = (n) => String(n).padStart(2, "0");
    return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
  }

  async function queryQuota(account, force = false) {
    const cached = quotaCache.get(account.id);
    if (!force && cached && Date.now() - cached.queried_at < 60000) return cached;
    if (quotaRequests.has(account.id)) return quotaRequests.get(account.id);
    const pending = requestJson(`/api/auth/devin-accounts/${encodeURIComponent(account.id)}/quota`)
      .then((result) => {
        if (!Number.isFinite(result.overage_balance)) throw new Error("额度接口返回无效余额");
        result.queried_at = Date.now();
        quotaCache.set(account.id, result);
        if (typeof window.recordAccountRefreshed === "function") {
          window.recordAccountRefreshed("devin", account.id, result.queried_at);
        }
        return result;
      }).finally(() => quotaRequests.delete(account.id));
    quotaRequests.set(account.id, pending);
    return pending;
  }

  function updateResetCountdown(line) {
    const resetAt = Number(line.dataset.devinResetAt);
    const remaining = resetAt - Date.now();
    let countdown = "已到重设时间，请刷新额度";
    if (remaining > 0) {
      const minutes = Math.ceil(remaining / 60000);
      const days = Math.floor(minutes / 1440);
      const hours = Math.floor((minutes % 1440) / 60);
      const mins = minutes % 60;
      countdown = `还有${days ? `${days}天 ` : ""}${hours ? `${hours}小时 ` : ""}${mins}分钟`;
    }
    line.textContent = `${line.dataset.devinResetLabel}：${countdown}；重设时间：${formatDevinDateTime(resetAt)}`;
  }

  function quotaResetLine(label, resetAt) {
    const line = document.createElement("div");
    line.className = "usage-window-line";
    const timestamp = typeof resetAt === "string" && resetAt.trim() ? Date.parse(resetAt) : NaN;
    if (!Number.isFinite(timestamp)) {
      line.textContent = `${label}：重设时间未知`;
      return line;
    }
    line.dataset.devinResetAt = String(timestamp);
    line.dataset.devinResetLabel = label;
    updateResetCountdown(line);
    return line;
  }

  const subscriptionCache = new Map();
  const subscriptionRequests = new Map();

  async function querySubscription(account, force = false) {
    const cached = subscriptionCache.get(account.id) || account.subscription;
    if (!force && cached && Date.now() - cached.queried_at < 60000) return cached;
    if (subscriptionRequests.has(account.id)) return subscriptionRequests.get(account.id);
    const pending = requestJson(`/api/auth/devin-accounts/${encodeURIComponent(account.id)}/subscription/refresh`, { method: "POST" })
      .then((result) => {
        if (!result || typeof result.status !== "string" || typeof result.slug !== "string") {
          throw new Error("订阅接口返回无效数据");
        }
        subscriptionCache.set(account.id, result);
        account.subscription = result;
        return result;
      }).catch((error) => {
        subscriptionCache.delete(account.id);
        throw error;
      }).finally(() => subscriptionRequests.delete(account.id));
    subscriptionRequests.set(account.id, pending);
    return pending;
  }

  function subscriptionCell(account) {
    const cell = document.createElement("td");
    cell.className = "mono-text credits-cell";
    if (account.clientType === "desktop") {
      cell.textContent = "不支持查询";
      return cell;
    }
    const value = document.createElement("span");
    const detail = document.createElement("div");
    detail.className = "usage-window-line";
    const refresh = document.createElement("button");
    refresh.type = "button";
    refresh.className = "usage-mini-action";
    refresh.textContent = "刷新";
    refresh.setAttribute("aria-label", `查询并更新 ${account.name || account.id} 的套餐到期时间`);
    let lastSuccess = subscriptionCache.get(account.id) || account.subscription;
    function render(result) {
      const statuses = { active: "有效", canceled: "已取消", past_due: "待付款", trialing: "试用中", unpaid: "未付款", incomplete: "未完成", incomplete_expired: "已失效" };
      value.textContent = `${result.slug} · ${statuses[result.status] || result.status} `;
      const changingPlan = result.end_of_cycle_slug && result.end_of_cycle_slug !== result.slug;
      const seconds = changingPlan ? result.current_period_end : (result.cancel_at || result.current_period_end);
      const label = changingPlan ? "套餐变更时间" : result.cancel_at ? "套餐结束时间" : result.status === "active" ? "续费时间" : "本期结束时间";
      if (Number.isFinite(seconds) && seconds > 0) {
        const at = seconds * 1000;
        const remaining = at - Date.now();
        const countdown = remaining > 0 ? `还有 ${Math.ceil(remaining / 86400000)} 天` : "已到期，请刷新订阅状态";
        detail.textContent = `${label}：${formatDevinDateTime(at)}（${countdown}）`;
        if (changingPlan) detail.textContent += `；转为 ${result.end_of_cycle_slug}`;
      } else {
        detail.textContent = "接口未提供套餐到期时间";
      }
      value.title = `查询时间：${formatDevinDateTime(result.queried_at)}`;
    }
    async function update(force) {
      refresh.disabled = true;
      if (lastSuccess) render(lastSuccess);
      else { value.textContent = "查询中… "; detail.textContent = ""; }
      try {
        lastSuccess = await querySubscription(account, force);
        render(lastSuccess);
      } catch (error) {
        if (lastSuccess) {
          render(lastSuccess);
          value.textContent = `查询失败；上次结果：${value.textContent}`;
        } else {
          value.textContent = "查询失败 ";
          detail.textContent = "";
        }
        value.title = `${error.message || "套餐查询失败"}${lastSuccess ? `；上次成功：${formatDevinDateTime(lastSuccess.queried_at)}` : ""}`;
      } finally {
        refresh.disabled = false;
      }
    }
    refresh.addEventListener("click", () => update(true));
    cell.append(value, refresh, detail);
    void update(false);
    return cell;
  }

  function quotaCell(account, included = false) {
    const cell = document.createElement("td");
    cell.hidden = account.kind !== "devin";
    cell.className = "mono-text";
    if (cell.hidden) return cell;
    if (account.clientType === "desktop") {
      cell.textContent = "不支持查询";
      cell.title = "Desktop 记录仅含登录摘要，不含额度接口会话凭据";
      return cell;
    }
    const value = document.createElement("span");
    const resetDetails = document.createElement("div");
    const refresh = document.createElement("button");
    refresh.type = "button";
    refresh.className = "usage-mini-action";
    refresh.textContent = "刷新";
    refresh.setAttribute("aria-label", `刷新 ${account.name || account.id} 的${included ? "套餐内" : "套餐外"}剩余额度`);
    async function update(force) {
      refresh.disabled = true;
      value.textContent = "查询中… ";
      resetDetails.replaceChildren();
      try {
        const result = await queryQuota(account, force);
        if (included) {
          const windows = [];
          if (result.is_quota_plan === false) {
            value.textContent = "非额度套餐 ";
          } else if (result.has_quota_allocation === false) {
            value.textContent = "未分配额度 ";
          } else {
            if (result.hide_daily_quota !== true && Number.isFinite(result.daily_percentage)) {
              windows.push(`日 ${Math.max(0, 100 - result.daily_percentage).toFixed(0)}%`);
              resetDetails.append(quotaResetLine("日额度", result.daily_reset_at));
            }
            if (Number.isFinite(result.weekly_percentage)) {
              windows.push(`周 ${Math.max(0, 100 - result.weekly_percentage).toFixed(0)}%`);
              resetDetails.append(quotaResetLine("周额度", result.weekly_reset_at));
            }
            value.textContent = windows.length ? `${windows.join(" / ")} ` : "无套餐内额度数据 ";
          }
          value.title = `套餐内剩余比例；日重置：${result.daily_reset_at || "未知"}；周重置：${result.weekly_reset_at || "未知"}`;
        } else {
          value.textContent = `${result.overage_balance.toFixed(2)} `;
          value.title = "套餐外余额（overage_balance），保留接口原始单位";
        }
        value.title += `；查询时间：${formatDevinDateTime(result.queried_at)}`;
      } catch (error) {
        value.textContent = "查询失败 ";
        value.title = error.message || "额度查询失败";
      } finally {
        refresh.disabled = false;
      }
    }
    refresh.addEventListener("click", () => update(true));
    cell.append(value, refresh, resetDetails);
    void update(false);
    return cell;
  }

  function setAddStatus(message, tone = "info") {
    showStatus(message, tone, "devin-add-status");
  }

  function openAddDialog() {
    const dialog = el("devin-add-dialog");
    if (!dialog) return;
    ["devin-add-token", "devin-add-email", "devin-add-org-id", "devin-add-plan", "devin-add-name"].forEach((id) => {
      if (el(id)) el(id).value = "";
    });
    setAddStatus("", "muted");
    if (!dialog.open) dialog.showModal();
  }

  function closeAddDialog() {
    const dialog = el("devin-add-dialog");
    if (dialog?.open) dialog.close();
  }

  async function manualAdd(event) {
    event?.preventDefault?.();
    const token = el("devin-add-token")?.value.trim() || "";
    const email = el("devin-add-email")?.value.trim() || "";
    const orgId = el("devin-add-org-id")?.value.trim() || "";
    const plan = el("devin-add-plan")?.value.trim() || "";
    const name = el("devin-add-name")?.value.trim() || "";
    if (!token) {
      setAddStatus("请提供 API Key 或 Token。", "warn");
      return;
    }
    setAddStatus("正在添加账号…", "info");
    try {
      const meta = await requestJson("/api/auth/devin-accounts", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ token, email, org_id: orgId, plan, name }),
      });
      setAddStatus(`已添加账号 ${meta?.name || meta?.email || meta?.id || ""}。`, "ok");
      closeAddDialog();
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
    } catch (error) {
      setAddStatus(error.message || "添加账号失败。", "warn");
    }
  }

  async function switchAccount(account) {
    showStatus(`正在切换 Devin CLI 账号到 ${account.name}…`, "info");
    try {
      await requestJson(
        `/api/auth/devin-accounts/${encodeURIComponent(account.id)}/switch`,
        { method: "PUT" },
      );
      showStatus(`已应用到 Devin CLI：${account.name}。新启动的 CLI 使用此账号，已运行的 CLI 需重新启动；Desktop 登录需单独管理。`, "ok");
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
      return true;
    } catch (error) {
      showStatus(error.message || "切换 Devin 账号失败。", "warn");
      return false;
    }
  }

  async function deleteAccount(accountId) {
    try {
      await requestJson(
        `/api/auth/devin-accounts/${encodeURIComponent(accountId)}`,
        { method: "DELETE" },
      );
      showStatus("已删除 Devin 账号。", "ok");
      return true;
    } catch (error) {
      showStatus(error.message || "删除 Devin 账号失败。", "warn");
      throw error;
    }
  }

  async function importCurrentAccount() {
    showStatus("正在从系统读取当前 Devin CLI 配置…", "info");
    try {
      const result = await requestJson("/api/auth/devin-accounts/import-current", {
        method: "POST",
      });
      showStatus(`已成功导入当前 Devin CLI 账号：${result.name || result.email || result.id}`, "ok");
      closeAddDialog();
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
    } catch (error) {
      showStatus(error.message || "读取当前 Devin CLI 配置失败。", "warn");
      setAddStatus(error.message || "读取当前 Devin CLI 配置失败。", "warn");
    }
  }

  async function detectDesktop(event) {
    event?.preventDefault?.();
    showStatus("正在读取 webClx 所在主机的 Devin Desktop 登录数据库…", "info");
    try {
      const result = await requestJson("/api/auth/devin-accounts/detect-desktop", { method: "POST" });
      const account = result?.account || {};
      const identity = account.email || account.id || "本机 Desktop";
      const state = account.logged_in ? "检测到登录字段" : "未检测到完整登录字段";
      showStatus(
        `${state}：${identity}。来源是 webClx 服务器本机，尚未验证服务端登录有效性。`,
        account.logged_in ? "ok" : "warn",
      );
    } catch (error) {
      showStatus(error.message || "读取本机 Devin Desktop 配置失败。", "warn");
    }
  }

  async function importDesktopCurrent(event) {
    event?.preventDefault?.();
    showStatus("正在把 webClx 所在主机的 Desktop 账号类型加入列表…", "info");
    try {
      const result = await requestJson("/api/auth/devin-accounts/import-desktop-current", {
        method: "POST",
      });
      const account = result?.account || {};
      showStatus(
        `已添加 Desktop 账号记录：${account.email || account.name || account.id}。登录凭据未复制到 CLI，也未发送到浏览器。`,
        "ok",
      );
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
    } catch (error) {
      showStatus(error.message || "添加本机 Devin Desktop 账号失败。", "warn");
    }
  }

  async function desktopAction(account, action) {
    const name = account?.name || account?.email || "Devin Desktop";
    if (action === "detect") {
      showStatus(`正在核验 ${name} 的 Desktop 配置字段…`, "info");
      try {
        const result = await requestJson(
          "/api/auth/devin-accounts/detect-desktop",
          { method: "POST" },
        );
        const detected = result?.account || {};
        showStatus(
          `${detected.logged_in ? "检测到完整登录字段" : "登录字段不完整"}：${detected.email || detected.id || name}。字段存在不等于服务端请求成功。`,
          detected.logged_in ? "ok" : "warn",
        );
      } catch (error) {
        showStatus(error.message || "Desktop 登录核验失败。", "warn");
      }
      return;
    }
    const command = action === "backup"
      ? "python3 scripts/devin/desktop-auth-backup.py backup /安全目录/login.gpg"
      : "python3 scripts/devin/desktop-auth-backup.py restore /安全目录/login.gpg";
    const requirement = action === "restore" ? "恢复前必须正常退出 Devin Desktop。" : "";
    showStatus(
      `Desktop ${action === "backup" ? "备份" : "恢复"}需在终端交互输入 GPG 密码：${command}。${requirement} 网页未执行磁盘写入。`,
      "info",
    );
  }

  async function refreshCliOptions() {
    try {
      const res = await requestJson("/api/auth/devin-options");
      const bypassBox = el("devin-option-bypass");
      if (bypassBox && typeof res.bypass === "boolean") {
        bypassBox.checked = res.bypass;
      }
    } catch {
      // ignore
    }
  }

  async function saveCliOptions() {
    const bypassBox = el("devin-option-bypass");
    if (!bypassBox) return;
    const bypass = bypassBox.checked === true;
    const note = el("devin-options-note");
    try {
      await requestJson("/api/auth/devin-options", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ bypass }),
      });
      const msg = bypass
        ? "已开启全局 Bypass：允许所有操作，禁止询问。"
        : "已关闭全局 Bypass：恢复正常权限询问。";
      showStatus(msg, "ok");
      if (note) {
        note.textContent = bypass ? "已开启全局 Bypass" : "已关闭全局 Bypass";
        note.dataset.tone = "ok";
      }
    } catch (error) {
      const msg = error.message || "保存 Devin 选项失败。";
      showStatus(msg, "warn");
      if (note) {
        note.textContent = msg;
        note.dataset.tone = "warn";
      }
      await refreshCliOptions();
    }
  }

  async function detectCurrent() {
    showStatus("正在从本地 credentials.toml 重新读取当前 Devin CLI 账号…", "info");
    try {
      const res = await requestJson("/api/auth/devin-accounts/detect-current", { method: "POST" });
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
      if (res?.matched && res?.account) {
        const name = res.account.name || res.account.email || res.account.id;
        showStatus(`已重新读取本地 Devin CLI 当前登录账号：${name}（${res.account.plan || "Devin"}）`, "ok");
      } else {
        showStatus("已重新读取本地 Devin CLI 配置：当前未检测到登录凭据。", "info");
      }
    } catch (error) {
      showStatus(`重新读取当前 Devin CLI 账号失败：${error.message}`, "warn");
    }
  }

  window.WebClxDevinManager = Object.freeze({
    loadAccounts,
    queryQuota,
    getQuotaCache: (id) => quotaCache.get(id),
    quotaCell,
    subscriptionCell,
    openAddDialog,
    closeAddDialog,
    switchAccount,
    deleteAccount,
    importCurrentAccount,
    detectDesktop,
    importDesktopCurrent,
    desktopAction,
    detectCurrent,
    refreshCliOptions,
    saveCliOptions,
  });

  window.addEventListener("DOMContentLoaded", () => {
    // One timer scans only connected cells; table redraws never retain old rows.
    window.setInterval(() => {
      document.querySelectorAll("[data-devin-reset-at]").forEach(updateResetCountdown);
    }, 30000);
    el("codex-account-add-devin")?.addEventListener("click", openAddDialog);
    el("devin-import-current-toolbar")?.addEventListener("click", importCurrentAccount);
    el("devin-import-current-btn")?.addEventListener("click", importCurrentAccount);
    el("devin-detect-desktop")?.addEventListener("click", detectDesktop);
    el("devin-import-desktop-toolbar")?.addEventListener("click", importDesktopCurrent);
    el("devin-detect-current")?.addEventListener("click", detectCurrent);
    el("devin-option-bypass")?.addEventListener("change", saveCliOptions);
    el("devin-add-close")?.addEventListener("click", closeAddDialog);
    el("devin-add-form")?.addEventListener("submit", manualAdd);
    el("devin-add-dialog")?.addEventListener("cancel", (event) => {
      event.preventDefault();
      closeAddDialog();
    });
  });
})();
