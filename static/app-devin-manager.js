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

  async function loadAccounts() {
    try {
      const response = await requestJson("/api/auth/devin-accounts");
      return Array.isArray(response) ? response : [];
    } catch {
      return [];
    }
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
      showStatus(`已切换到 Devin 账号 ${account.name}。`, "ok");
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
    showStatus("正在从系统读取当前 Devin 配置…", "info");
    try {
      const result = await requestJson("/api/auth/devin-accounts/import-current", {
        method: "POST",
      });
      showStatus(`已成功导入当前 Devin 账号：${result.name || result.email || result.id}`, "ok");
      closeAddDialog();
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
    } catch (error) {
      showStatus(error.message || "读取当前 Devin 配置失败。", "warn");
      setAddStatus(error.message || "读取当前 Devin 配置失败。", "warn");
    }
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
    showStatus("正在从本地 credentials.toml 重新读取当前 Devin 账号…", "info");
    try {
      const res = await requestJson("/api/auth/devin-accounts/detect-current", { method: "POST" });
      if (typeof loadCodexAccounts === "function") await loadCodexAccounts("devin");
      if (res?.matched && res?.account) {
        const name = res.account.name || res.account.email || res.account.id;
        showStatus(`已重新读取本地 Devin 当前登录账号：${name}（${res.account.plan || "Devin"}）`, "ok");
      } else {
        showStatus("已重新读取本地 Devin 配置：当前未检测到登录凭据。", "info");
      }
    } catch (error) {
      showStatus(`重新读取当前 Devin 账号失败：${error.message}`, "warn");
    }
  }

  window.WebClxDevinManager = Object.freeze({
    loadAccounts,
    openAddDialog,
    closeAddDialog,
    switchAccount,
    deleteAccount,
    importCurrentAccount,
    detectCurrent,
    refreshCliOptions,
    saveCliOptions,
  });

  window.addEventListener("DOMContentLoaded", () => {
    el("codex-account-add-devin")?.addEventListener("click", openAddDialog);
    el("devin-import-current-toolbar")?.addEventListener("click", importCurrentAccount);
    el("devin-import-current-btn")?.addEventListener("click", importCurrentAccount);
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
