(function () {
  const elements = {};
  let connectedToken = "";
  let discoveredAccounts = [];

  const DEFAULT_PROFILES = [
    {
      id: "local",
      name: "本地 sub2api",
      baseUrl: "http://127.0.0.1:PORT",
      email: "admin@example.invalid",
      password: "",
    },
  ];

  function getProfiles() {
    return DEFAULT_PROFILES;
  }

  function applyProfile(profileId) {
    const profiles = getProfiles();
    const profile = profiles.find((p) => p.id === profileId) || profiles[0];
    if (!profile) return;

    if (elements.profileSelect) elements.profileSelect.value = profile.id;
    if (elements.baseUrl) elements.baseUrl.value = profile.baseUrl || "";
    if (elements.email) elements.email.value = profile.email || "";
    if (elements.password) elements.password.value = profile.password || "";
  }

  function renderProfileOptions() {
    if (!elements.profileSelect) return;
    elements.profileSelect.innerHTML = "";
    const profiles = getProfiles();
    profiles.forEach((p) => {
      const opt = document.createElement("option");
      opt.value = p.id;
      opt.textContent = p.name;
      elements.profileSelect.appendChild(opt);
    });
    if (profiles.length) {
      elements.profileSelect.value = profiles[0].id;
      applyProfile(profiles[0].id);
    }
  }

  function init(deps) {
    const {
      requestJson,
      updateStatus,
      sub2apiManagerDialogEl,
      sub2apiManagerFormEl,
      sub2apiProfileSelectEl,
      sub2apiBaseUrlInputEl,
      sub2apiEmailInputEl,
      sub2apiPasswordInputEl,
      sub2apiTotpSectionEl,
      sub2apiTotpCodeInputEl,
      sub2apiTempTokenInputEl,
      sub2apiConnectButtonEl,
      sub2apiManagerStatusEl,
      sub2apiAccountsSectionEl,
      sub2apiAccountListEl,
      sub2apiImportSelectedButtonEl,
      sub2apiManagerCancelButtonEl,
    } = deps;

    Object.assign(elements, {
      dialog: sub2apiManagerDialogEl,
      form: sub2apiManagerFormEl,
      profileSelect: sub2apiProfileSelectEl,
      baseUrl: sub2apiBaseUrlInputEl,
      email: sub2apiEmailInputEl,
      password: sub2apiPasswordInputEl,
      totpSection: sub2apiTotpSectionEl,
      totpCode: sub2apiTotpCodeInputEl,
      tempToken: sub2apiTempTokenInputEl,
      connectButton: sub2apiConnectButtonEl,
      status: sub2apiManagerStatusEl,
      accountsSection: sub2apiAccountsSectionEl,
      accountList: sub2apiAccountListEl,
      importSelected: sub2apiImportSelectedButtonEl,
      cancelButton: sub2apiManagerCancelButtonEl,
    });

    renderProfileOptions();
    resetForm();
  }

  function resetForm() {
    renderProfileOptions();
    if (elements.totpCode) elements.totpCode.value = "";
    if (elements.totpSection) elements.totpSection.hidden = true;
    if (elements.accountsSection) elements.accountsSection.hidden = true;
    if (elements.accountList) elements.accountList.innerHTML = "";
    const selectAllEl = document.getElementById("sub2api-select-all");
    if (selectAllEl) {
      selectAllEl.checked = false;
      selectAllEl.indeterminate = false;
    }
    if (elements.importSelected) {
      elements.importSelected.textContent = "导入已选账号";
    }
    connectedToken = "";
    discoveredAccounts = [];
    setStatus("", "muted");
  }

  function setStatus(message, tone = "muted") {
    if (!elements.status) return;
    if (!message) {
      elements.status.hidden = true;
      elements.status.textContent = "";
      elements.status.dataset.tone = "muted";
      return;
    }
    elements.status.hidden = false;
    elements.status.textContent = message;
    elements.status.dataset.tone = tone;
  }

  function open() {
    resetForm();
    if (typeof elements.dialog?.showModal === "function") {
      if (!elements.dialog.open) elements.dialog.showModal();
    } else {
      elements.dialog?.setAttribute("open", "");
    }
  }

  function close() {
    if (elements.dialog?.open) elements.dialog.close();
  }

  function bindEvents() {
    if (elements.profileSelect) {
      elements.profileSelect.addEventListener("change", (e) => {
        applyProfile(e.target.value);
      });
    }
    const selectAllEl = document.getElementById("sub2api-select-all");
    if (selectAllEl) {
      selectAllEl.addEventListener("change", () => {
        const checked = selectAllEl.checked;
        const checkboxes = elements.accountList?.querySelectorAll(".sub2api-account-checkbox") || [];
        checkboxes.forEach((cb) => {
          cb.checked = checked;
        });
        updateSelectedCount();
      });
    }
    if (elements.connectButton) {
      elements.connectButton.addEventListener("click", handleConnect);
    }
    if (elements.importSelected) {
      elements.importSelected.addEventListener("click", handleImportSelected);
    }
    if (elements.cancelButton) {
      elements.cancelButton.addEventListener("click", close);
    }
    const viewTabs = document.querySelectorAll(".sub2api-view-tabs .tab-button");
    viewTabs.forEach((btn) => {
      btn.addEventListener("click", () => switchView(btn.dataset.view || "normal"));
    });
    if (elements.form) {
      elements.form.addEventListener("submit", (event) => event.preventDefault());
    }
  }

  async function handleConnect() {
    const baseUrl = elements.baseUrl?.value.trim();
    const email = elements.email?.value.trim();
    const password = elements.password?.value;
    const totpCode = elements.totpCode?.value.trim();
    const tempToken = elements.tempToken?.value.trim();

    if (!baseUrl) {
      setStatus("请输入 sub2api 服务器地址。", "warn");
      return;
    }

    if (!connectedToken && !tempToken && (!email || !password)) {
      setStatus("请输入管理员邮箱和密码。", "warn");
      return;
    }

    elements.connectButton.disabled = true;
    setStatus("正在连接 sub2api…", "info");

    try {
      const payload = { base_url: baseUrl };
      if (tempToken && totpCode) {
        payload.temp_token = tempToken;
        payload.totp_code = totpCode;
      } else {
        payload.email = email;
        payload.password = password;
      }

      const loginResult = await requestJson("/api/auth/sub2api/sync", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ...payload,
          dry_run: true,
          list_only: true,
          recycled: currentView === "archived",
        }),
      });

      if (loginResult.token) {
        connectedToken = loginResult.token;
      }

      if (Array.isArray(loginResult.accounts)) {
        discoveredAccounts = loginResult.accounts;
        if (!discoveredAccounts.length) {
          setStatus("sub2api 中没有可导入的账号。", "warn");
        } else {
          renderAccounts();
          if (typeof attachPelicanHandlers === 'function') attachPelicanHandlers();
          if (elements.accountsSection) elements.accountsSection.hidden = false;
          setStatus(`已发现 ${discoveredAccounts.length} 个账号。`, "ok");
        }
      } else {
        setStatus("已连接，正在拉取账号列表…", "info");
        await loadAccounts(baseUrl, connectedToken);
        setStatus(`已发现 ${discoveredAccounts.length} 个账号。`, "ok");
      }
    } catch (error) {
      if (error?.message?.includes("TOTP")) {
        if (elements.totpSection) elements.totpSection.hidden = false;
        const match = error.message.match(/temp_token:\s*([^\s]+)/);
        if (match && elements.tempToken) {
          elements.tempToken.value = match[1];
        }
        setStatus("需要 TOTP 2FA 验证，请输入 6 位验证码。", "warn");
      } else {
        setStatus(error?.message || "连接 sub2api 失败。", "warn");
      }
    } finally {
      elements.connectButton.disabled = false;
    }
  }

  async function loadAccounts(baseUrl, token) {
    const result = await requestJson("/api/auth/sub2api/sync", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        base_url: baseUrl,
        token,
        list_only: true,
        recycled: currentView === "archived",
      }),
    });

    discoveredAccounts = result.accounts || [];
    if (!discoveredAccounts.length) {
      setStatus("sub2api 中没有可导入的账号。", "warn");
      return;
    }

    renderAccounts();
    if (typeof attachPelicanHandlers === 'function') attachPelicanHandlers();
    if (elements.accountsSection) elements.accountsSection.hidden = false;
  }

  function updateSelectedCount() {
    if (!elements.importSelected) return;
    const allBoxes = elements.accountList?.querySelectorAll(".sub2api-account-checkbox") || [];
    const checkedBoxes = elements.accountList?.querySelectorAll(".sub2api-account-checkbox:checked") || [];
    const count = checkedBoxes.length;

    elements.importSelected.textContent = count > 0 ? `导入已选账号 (${count})` : "导入已选账号";

    const selectAllEl = document.getElementById("sub2api-select-all");
    if (selectAllEl && allBoxes.length > 0) {
      selectAllEl.checked = count === allBoxes.length;
      selectAllEl.indeterminate = count > 0 && count < allBoxes.length;
    }
  }

  function renderAccounts() {
    if (!elements.accountList) return;
    elements.accountList.innerHTML = "";

    discoveredAccounts.forEach((account, index) => {
      const item = document.createElement("div");
      item.className = "sub2api-account-item";
      item.dataset.accountIndex = index;
      item.style.cursor = "pointer";

      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "sub2api-account-checkbox";
      checkbox.dataset.accountId = account.id;
      checkbox.checked = false; // 默认不勾选：需要一个勾选一个
      checkbox.addEventListener("change", updateSelectedCount);

      const info = document.createElement("div");
      info.className = "sub2api-account-info";

      const name = document.createElement("div");
      name.className = "sub2api-account-name";
      name.textContent = account.name || `账号 ${account.id}`;

      const meta = document.createElement("div");
      meta.className = "sub2api-account-meta";
      meta.textContent = `${account.platform} · ${account.kind}${account.base_url ? ` · ${account.base_url}` : ""}`;

      info.append(name, meta);

      const usage = document.createElement("div");
      usage.className = "sub2api-account-usage";
      usage.textContent = "用量加载中…";
      usage.dataset.accountId = account.id;
      usage.dataset.baseUrl = elements.baseUrl?.value.trim() || "";
      usage.dataset.platform = account.platform || "";
      usage.dataset.kind = account.kind || "";
      usage.addEventListener("click", (e) => {
        e.stopPropagation();
        handleQueryUsage(usage);
      });

      const actionBtn = document.createElement("button");
      actionBtn.className = "button secondary sub2api-archive-btn";
      actionBtn.type = "button";
      actionBtn.textContent = currentView === "normal" ? "归档" : "恢复";
      actionBtn.dataset.accountId = account.id;
      actionBtn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        handleArchiveToggle(account.id);
      });

      item.addEventListener("click", (e) => {
        if (e.target.closest("button") || e.target.closest(".sub2api-account-usage") || e.target === checkbox) {
          return;
        }
        checkbox.checked = !checkbox.checked;
        updateSelectedCount();
      });

      item.append(checkbox, info, usage, actionBtn);
      elements.accountList.appendChild(item);
    });

    updateSelectedCount();
    void autoLoadUsageWindows();
  }

  // 渲染用量窗口：与 sub2api 账号表格一致，展示 5h / 7d / 7dS / 7dF
  // 各窗口利用率，颜色按最高利用率分档；reset 时间、采样方式与窗口
  // 统计放入 title。
  function formatUsageWindowPart(label, progress) {
    if (!progress) return null;
    const pct = Math.round(progress.utilization ?? 0);
    return `${label} ${pct}%`;
  }

  function formatUsageReset(progress) {
    const reset = progress?.resets_at;
    if (!reset) return "";
    const time = new Date(reset);
    if (Number.isNaN(time.getTime())) return "";
    const pad = (n) => String(n).padStart(2, "0");
    return `${pad(time.getMonth() + 1)}-${pad(time.getDate())} ${pad(time.getHours())}:${pad(time.getMinutes())}`;
  }

  function renderUsageInfo(usageEl, usage) {
    const windows = [
      ["5h", usage.five_hour],
      ["7d", usage.seven_day],
      ["7d S", usage.seven_day_sonnet],
      ["7d F", usage.seven_day_fable],
    ];
    const parts = windows
      .map(([label, progress]) => formatUsageWindowPart(label, progress))
      .filter(Boolean);

    if (usage.error) {
      usageEl.textContent = parts.length ? parts.join(" · ") : "降级";
    } else {
      usageEl.textContent = parts.length ? parts.join(" · ") : "无额度数据";
    }

    const maxPct = Math.max(
      usage.five_hour?.utilization ?? 0,
      usage.seven_day?.utilization ?? 0,
      usage.seven_day_sonnet?.utilization ?? 0,
      usage.seven_day_fable?.utilization ?? 0,
    );
    usageEl.classList.remove("sub2api-usage-ok", "sub2api-usage-warn", "sub2api-usage-danger");
    usageEl.classList.add(
      maxPct >= 90 ? "sub2api-usage-danger" : maxPct >= 70 ? "sub2api-usage-warn" : "sub2api-usage-ok",
    );

    const titleLines = [];
    for (const [label, progress] of windows) {
      if (!progress) continue;
      const stats = progress.window_stats;
      const statText = stats
        ? `，已用 ${stats.requests ?? 0} 请求 / ${stats.tokens ?? 0} token`
        : "";
      const reset = formatUsageReset(progress);
      titleLines.push(
        `${label}: ${Math.round(progress.utilization ?? 0)}%${reset ? `，重置 ${reset}` : ""}${statText}`,
      );
    }
    if (usage.source === "passive") titleLines.push("被动采样");
    if (usage.updated_at) titleLines.push(`采样时间 ${usage.updated_at}`);
    if (usage.error) titleLines.push(`错误：${usage.error}`);
    titleLines.push("单击主动查询，右键鹈鹕测智");
    usageEl.title = titleLines.join("\n");
  }

  // 不支持用量查询的账号类型（sub2api 对 apikey 账号返回
  // "does not support usage query"），展示为占位符而不是报错。
  function isUnsupportedUsageError(message) {
    return typeof message === "string" && message.includes("does not support usage query");
  }

  // 学习 sub2api 账号表格：列表渲染后自动加载用量窗口。优先调用批量
  // 接口（sub2api 内部对 Anthropic 走被动采样、其余受限并发主动探测）；
  // 批量接口不可用时按 sub2api 前端的做法逐账号 passive 兜底。
  async function autoLoadUsageWindows() {
    const cells = Array.from(elements.accountList?.querySelectorAll(".sub2api-account-usage") || []);
    const baseUrl = elements.baseUrl?.value.trim();
    if (!cells.length || !baseUrl || !connectedToken) return;

    const accountIds = cells
      .map((cell) => parseInt(cell.dataset.accountId, 10))
      .filter(Boolean);
    const byAccountId = new Map(
      cells.map((cell) => [String(cell.dataset.accountId), cell]),
    );

    try {
      const result = await requestJson("/api/auth/sub2api/usage-batch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ base_url: baseUrl, token: connectedToken, account_ids: accountIds }),
      });
      if (result.ok && (result.usage || result.errors)) {
        for (const [id, cell] of byAccountId) {
          const usage = result.usage?.[id];
          if (usage) {
            renderUsageInfo(cell, usage);
          } else {
            const message = result.errors?.[id];
            cell.textContent = isUnsupportedUsageError(message) ? "-" : (message || "无额度数据");
            if (message) cell.title = message;
          }
        }
        return;
      }
      throw new Error(result.error || "批量额度接口不可用");
    } catch {
      loadUsageWindowsIndividually(cells, baseUrl);
    }
  }

  // 兜底：模拟 sub2api AccountUsageCell 的挂载行为——Anthropic
  // OAuth/SetupToken 先取 passive（不触碰上游），OpenAI OAuth 取默认
  // 主动查询（sub2api 侧有缓存），其他类型跳过。
  function passiveUsageSupported(cell) {
    const platform = (cell.dataset.platform || "").toLowerCase();
    const kind = (cell.dataset.kind || "").toLowerCase();
    return (
      (platform === "anthropic" || platform === "claude") &&
      (kind === "oauth" || kind === "setup-token" || kind === "setuptoken")
    );
  }

  function activeUsageSupported(cell) {
    const platform = (cell.dataset.platform || "").toLowerCase();
    const kind = (cell.dataset.kind || "").toLowerCase();
    return platform === "openai" && kind === "oauth";
  }

  async function loadUsageWindowsIndividually(cells, baseUrl) {
    const pending = cells.filter(
      (cell) => passiveUsageSupported(cell) || activeUsageSupported(cell),
    );
    const skipped = cells.filter(
      (cell) => !passiveUsageSupported(cell) && !activeUsageSupported(cell),
    );
    skipped.forEach((cell) => {
      cell.textContent = "-";
    });

    const concurrency = 4;
    let cursor = 0;
    async function worker() {
      while (cursor < pending.length) {
        const cell = pending[cursor++];
        const source = passiveUsageSupported(cell) ? "passive" : undefined;
        await queryUsageInto(cell, baseUrl, { source });
      }
    }
    await Promise.all(
      Array.from({ length: Math.min(concurrency, pending.length) }, worker),
    );
  }

  async function queryUsageInto(usageEl, baseUrl, { source, force = false } = {}) {
    const accountId = parseInt(usageEl.dataset.accountId, 10);
    if (!accountId) return;
    try {
      const result = await requestJson("/api/auth/sub2api/usage", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          base_url: baseUrl,
          token: connectedToken,
          account_id: accountId,
          source,
          force,
        }),
      });
      if (!result.ok || !result.usage) {
        usageEl.textContent = isUnsupportedUsageError(result.error)
          ? "-"
          : (result.error || "查询失败");
        return;
      }
      renderUsageInfo(usageEl, result.usage);
    } catch (error) {
      usageEl.textContent = error?.message || "查询失败";
    }
  }

  async function handleQueryUsage(usageEl) {
    const baseUrl = usageEl.dataset.baseUrl;
    if (!usageEl.dataset.accountId || !baseUrl || !connectedToken) return;
    usageEl.textContent = "查询中…";
    await queryUsageInto(usageEl, baseUrl, { force: true });
  }

  async function handleImportSelected() {
    if (!discoveredAccounts.length) {
      setStatus("当前视图没有可导入的账号。", "warn");
      return;
    }

    const checkedBoxes = Array.from(elements.accountList?.querySelectorAll(".sub2api-account-checkbox:checked") || []);
    const accountIds = checkedBoxes
      .map((cb) => parseInt(cb.dataset.accountId, 10))
      .filter(Boolean);

    if (!accountIds.length) {
      setStatus("请先勾选需要导入的账号。", "warn");
      return;
    }

    elements.importSelected.disabled = true;
    setStatus(`正在导入选中的 ${accountIds.length} 个账号…`, "info");

    try {
      const baseUrl = elements.baseUrl?.value.trim();
      const result = await requestJson("/api/auth/sub2api/sync", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          base_url: baseUrl,
          token: connectedToken,
          import_selected: accountIds,
        }),
      });

      const oauthCount = result.oauth_saved ?? 0;
      const apiCount = result.api_saved ?? 0;
      const claudeCount = result.claude_saved ?? 0;
      let msg = "";
      if (oauthCount > 0 || apiCount > 0 || claudeCount > 0) {
        const parts = [];
        if (oauthCount > 0) parts.push(`OAuth ${oauthCount} 个`);
        if (apiCount > 0) parts.push(`API ${apiCount} 个`);
        if (claudeCount > 0) parts.push(`Claude ${claudeCount} 个`);
        msg = `导入完成：${parts.join("，")}。`;
      } else {
        msg = `导入完成：Codex ${result.codex_saved || 0} 个，Claude ${result.claude_saved || 0} 个。`;
      }
      setStatus(msg, "ok");

      if (typeof window.loadCodexAccounts === "function") {
        window.loadCodexAccounts();
      }
      if (typeof window.refreshAuthPanels === "function") {
        window.refreshAuthPanels();
      }
      if (typeof window.loadClaudePresets === "function") {
        window.loadClaudePresets();
      }
      setTimeout(close, 2000);
    } catch (error) {
      setStatus(error?.message || "导入失败。", "warn");
    } finally {
      elements.importSelected.disabled = false;
      updateSelectedCount();
    }
  }

  
  async function handlePelicanTest(usageEl) {
    const accountId = parseInt(usageEl.dataset.accountId, 10);
    const baseUrl = usageEl.dataset.baseUrl;
    if (!accountId || !baseUrl || !connectedToken) return;

    usageEl.textContent = "鹈鹕测智中…";
    try {
      const result = await requestJson("/api/auth/sub2api/pelican", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          base_url: baseUrl,
          token: connectedToken,
          account_id: accountId,
        }),
      });

      if (!result.ok || !result.result) {
        usageEl.textContent = result.error || "测智失败";
        return;
      }

      const r = result.result;
      usageEl.textContent = r.downgraded ? `降智: ${r.reason}` : `通过: ${r.reason}`;
      usageEl.classList.remove("sub2api-usage-ok", "sub2api-usage-warn", "sub2api-usage-danger");
      usageEl.classList.add(r.downgraded ? "sub2api-usage-danger" : "sub2api-usage-ok");
      usageEl.title = `模型: ${r.response_model}\n延迟: ${r.latency_ms}ms\n${r.html ? "已生成 HTML" : "无 HTML"}`;
    } catch (error) {
      usageEl.textContent = error?.message || "测智失败";
    }
  }

  async function handleAutoActivate() {
    if (!discoveredAccounts.length || !connectedToken) {
      setStatus("请先连接并拉取账号。", "warn");
      return;
    }
    const accountIds = discoveredAccounts.map((a) => a.id).filter(Boolean);
    setStatus(`正在为 ${accountIds.length} 个账号设置每日 04:00 自动激活…`, "info");

    try {
      const result = await requestJson("/api/auth/sub2api/auto-activate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          base_url: elements.baseUrl?.value.trim(),
          token: connectedToken,
          account_ids: accountIds,
          execute_at: "04:00",
        }),
      });
      setStatus(
        `自动激活设置完成：成功 ${result.scheduled_count} 个` +
        (result.errors.length ? `，失败 ${result.errors.length} 个：${result.errors[0]}` : ""),
        result.errors.length ? "warn" : "ok",
      );
    } catch (error) {
      setStatus(error?.message || "设置自动激活失败。", "warn");
    }
  }

  // attach pelican on right-click or shift-click of usage cell
  function attachPelicanHandlers() {
    if (!elements.accountList) return;
    elements.accountList.querySelectorAll(".sub2api-account-usage").forEach((usageEl) => {
      usageEl.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        handlePelicanTest(usageEl);
      });
      usageEl.title = "单击主动查询，右键鹈鹕测智";
    });
  }

  let currentView = "normal"; // "normal" | "archived"

  function switchView(view) {
    currentView = view;
    updateViewButtons();
    if (connectedToken && elements.baseUrl?.value.trim()) {
      loadAccounts(elements.baseUrl.value.trim(), connectedToken);
    }
  }

  function updateViewButtons() {
    const normalBtn = document.getElementById("sub2api-view-normal");
    const archivedBtn = document.getElementById("sub2api-view-archived");
    if (normalBtn) {
      normalBtn.classList.toggle("active", currentView === "normal");
      normalBtn.setAttribute("aria-selected", currentView === "normal" ? "true" : "false");
    }
    if (archivedBtn) {
      archivedBtn.classList.toggle("active", currentView === "archived");
      archivedBtn.setAttribute("aria-selected", currentView === "archived" ? "true" : "false");
    }
  }

  async function handleArchiveToggle(accountId) {
    const baseUrl = elements.baseUrl?.value.trim();
    if (!baseUrl || !connectedToken || !accountId) return;

    const action = currentView === "normal" ? "recycle" : "restore";
    const actionLabel = currentView === "normal" ? "归档" : "恢复";
    setStatus(`正在${actionLabel}账号 ${accountId}…`, "info");

    try {
      const result = await requestJson(`/api/auth/sub2api/${action}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ base_url: baseUrl, token: connectedToken, account_id: accountId }),
      });
      if (!result.ok) {
        setStatus(result.error || `${actionLabel}失败`, "warn");
        return;
      }
      setStatus(`账号 ${accountId} 已${actionLabel}。`, "ok");
      // Refresh list
      await loadAccounts(baseUrl, connectedToken);
    } catch (error) {
      setStatus(error?.message || `${actionLabel}失败`, "warn");
    }
  }

  window.sub2apiManager = {
    init,
    bindEvents,
    open,
    close,
    handleAutoActivate,
  };
})();
