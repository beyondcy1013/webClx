(function () {
  const ENDPOINT = "/api/settings/preset-config/clipboard";

  async function copyTextWithFallback(text, clipboard, document) {
    if (clipboard?.writeText) {
      try {
        await clipboard.writeText(text);
        return true;
      } catch {
        // Insecure origins and denied permissions can still allow execCommand
        // while handling the original button click.
      }
    }
    if (!document?.body) {
      return false;
    }

    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.readOnly = true;
    textarea.setAttribute("aria-hidden", "true");
    textarea.style.cssText = "position:fixed;left:-9999px;top:0;width:1px;height:1px;opacity:0;pointer-events:none;";
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    try {
      return Boolean(document.execCommand?.("copy"));
    } catch {
      return false;
    } finally {
      textarea.remove();
    }
  }

  function createAccountClipboardManager(deps) {
    const {
      requestJson,
      updateStatus,
      refreshAuthPanels,
      sections = [],
      clipboard = globalThis.navigator?.clipboard,
      confirmImport = (message) => globalThis.window?.confirm(message) ?? false,
      documentRef = globalThis.document,
      windowRef = globalThis.window,
      copyText = (text) => copyTextWithFallback(text, clipboard, documentRef),
      openManualImport: customOpenManualImport = null,
    } = deps;
    const sectionMap = new Map(sections.map((entry) => [entry.section, entry]));
    let manualDialogElements = null;
    let manualDialogMode = "";
    let manualDialogSection = "";

    function getSection(section) {
      const entry = sectionMap.get(section);
      if (!entry) {
        throw new Error("未知的账号列表类别。");
      }
      return entry;
    }

    function setBusy(entry, busy) {
      if (entry.importButton) entry.importButton.disabled = busy;
      if (entry.exportButton) entry.exportButton.disabled = busy;
    }

    function reportStatus(entry, dialogStatus, message, tone) {
      updateStatus(entry.statusElement, message, tone);
      if (dialogStatus && dialogStatus !== entry.statusElement) {
        updateStatus(dialogStatus, message, tone);
      }
    }

    function closeManualDialog() {
      const elements = manualDialogElements;
      if (!elements) {
        return;
      }
      elements.textarea.value = "";
      manualDialogMode = "";
      manualDialogSection = "";
      if (elements.dialog.open && typeof elements.dialog.close === "function") {
        elements.dialog.close();
      } else {
        elements.dialog.removeAttribute("open");
      }
    }

    function ensureManualDialog() {
      if (manualDialogElements || !documentRef?.body) {
        return manualDialogElements;
      }

      const dialog = documentRef.createElement("dialog");
      dialog.className = "auth-import-dialog";
      dialog.id = "account-clipboard-manual-dialog";

      const form = documentRef.createElement("form");
      form.className = "auth-import-form";

      const header = documentRef.createElement("div");
      header.className = "panel-head wide";
      const headingWrap = documentRef.createElement("div");
      const label = documentRef.createElement("p");
      label.className = "section-label";
      label.textContent = "账号列表";
      const title = documentRef.createElement("h2");
      title.id = "account-clipboard-manual-title";
      headingWrap.append(label, title);
      header.appendChild(headingWrap);
      dialog.setAttribute("aria-labelledby", title.id);

      const textarea = documentRef.createElement("textarea");
      textarea.className = "dialog-textarea";
      textarea.spellcheck = false;

      const status = documentRef.createElement("div");
      status.className = "inline-status";
      status.dataset.tone = "muted";
      status.hidden = true;

      const actions = documentRef.createElement("div");
      actions.className = "toolbar dialog-actions";
      const cancelButton = documentRef.createElement("button");
      cancelButton.type = "button";
      cancelButton.className = "button secondary";
      cancelButton.textContent = "关闭";
      const submitButton = documentRef.createElement("button");
      submitButton.type = "submit";
      submitButton.className = "button primary";
      actions.append(cancelButton, submitButton);

      form.append(header, textarea, status, actions);
      dialog.appendChild(form);
      documentRef.body.appendChild(dialog);
      manualDialogElements = { dialog, form, title, textarea, status, submitButton };

      cancelButton.addEventListener("click", closeManualDialog);
      dialog.addEventListener("cancel", (event) => {
        event.preventDefault();
        closeManualDialog();
      });
      form.addEventListener("submit", async (event) => {
        event.preventDefault();
        if (manualDialogMode === "copy") {
          textarea.focus();
          textarea.select();
          updateStatus(status, "已选中导出内容。", "info");
          return;
        }
        if (!manualDialogSection) {
          return;
        }
        submitButton.disabled = true;
        const imported = await importRawText(manualDialogSection, textarea.value, status);
        submitButton.disabled = false;
        if (imported) {
          closeManualDialog();
        }
      });

      return manualDialogElements;
    }

    function showManualDialog({ mode, section = "", text = "" }) {
      const elements = ensureManualDialog();
      if (!elements) {
        return false;
      }
      const entry = section ? getSection(section) : null;
      manualDialogMode = mode;
      manualDialogSection = section;
      elements.title.textContent = mode === "copy"
        ? "导出账号列表"
        : `${entry?.label || ""} 账号列表导入`;
      elements.textarea.readOnly = mode === "copy";
      elements.textarea.value = text;
      elements.textarea.placeholder = mode === "copy" ? "" : "粘贴账号列表 JSON";
      elements.textarea.setAttribute(
        "aria-label",
        mode === "copy" ? "导出的账号列表 JSON" : "粘贴账号列表 JSON",
      );
      elements.submitButton.textContent = mode === "copy" ? "选中文本" : "导入";
      elements.status.hidden = true;
      elements.status.textContent = "";
      if (typeof elements.dialog.showModal === "function") {
        if (!elements.dialog.open) elements.dialog.showModal();
      } else {
        elements.dialog.setAttribute("open", "");
      }
      const focusTextarea = () => {
        elements.textarea.focus();
        if (mode === "copy") elements.textarea.select();
      };
      if (typeof windowRef?.requestAnimationFrame === "function") {
        windowRef.requestAnimationFrame(focusTextarea);
      } else {
        focusTextarea();
      }
      return true;
    }

    function openManualImportDialog(section, entry = getSection(section)) {
      const opened = showManualDialog({ mode: "import", section });
      if (opened) {
        updateStatus(entry.statusElement, "无法自动读取剪贴板，已打开手动粘贴窗口。", "info");
      } else {
        updateStatus(entry.statusElement, "无法打开账号列表粘贴窗口。", "warn");
      }
      return opened;
    }

    async function importRawText(section, rawText, dialogStatus = null) {
      const entry = getSection(section);
      const trimmedText = String(rawText || "").trim();
      if (!trimmedText) {
        reportStatus(entry, dialogStatus, "先粘贴账号列表 JSON。", "warn");
        return false;
      }

      let payload;
      try {
        payload = JSON.parse(trimmedText);
      } catch {
        reportStatus(entry, dialogStatus, "粘贴内容不是有效的 JSON。", "warn");
        return false;
      }
      const count = Array.isArray(payload?.accounts) ? payload.accounts.length : 0;
      if (!confirmImport(`确定从剪贴板导入 ${count} 个 ${entry.label} 账号吗？同 ID 账号会更新，其它账号保持不变。`)) {
        reportStatus(entry, dialogStatus, "已取消导入。", "muted");
        return false;
      }

      setBusy(entry, true);
      reportStatus(entry, dialogStatus, `正在导入 ${entry.label} 账号列表…`, "info");
      try {
        const result = await requestJson(`${ENDPOINT}/${encodeURIComponent(section)}/import`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: trimmedText,
        });
        await refreshAuthPanels();
        reportStatus(entry, dialogStatus, `已从剪贴板导入 ${result.imported_count} 个账号。`, "ok");
        return true;
      } catch (error) {
        reportStatus(entry, dialogStatus, error.message || "导入账号列表失败。", "warn");
        return false;
      } finally {
        setBusy(entry, false);
      }
    }

    async function exportSection(section) {
      const entry = getSection(section);
      const selectedIds = typeof entry.getSelectedIds === "function"
        ? entry.getSelectedIds().filter(Boolean)
        : [];
      if (selectedIds.length === 0) {
        updateStatus(entry.statusElement, "请先勾选要导出的账号。", "warn");
        return false;
      }

      setBusy(entry, true);
      updateStatus(entry.statusElement, `正在导出 ${entry.label} 账号列表…`, "info");
      try {
        const payload = await requestJson(`${ENDPOINT}/${encodeURIComponent(section)}/export`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ ids: selectedIds }),
        });
        const count = Array.isArray(payload.accounts) ? payload.accounts.length : 0;
        const text = `${JSON.stringify(payload, null, 2)}\n`;
        if (!await copyText(text)) {
          showManualDialog({ mode: "copy", text });
          updateStatus(entry.statusElement, "自动复制失败，已打开导出内容。", "warn");
          return false;
        }
        updateStatus(entry.statusElement, `已导出 ${count} 个账号到剪贴板。`, "ok");
        return true;
      } catch (error) {
        updateStatus(entry.statusElement, error.message || "导出账号列表失败。", "warn");
        return false;
      } finally {
        setBusy(entry, false);
      }
    }

    async function importSection(section) {
      const entry = getSection(section);
      if (!clipboard?.readText) {
        (customOpenManualImport || openManualImportDialog)(section, entry);
        return false;
      }

      setBusy(entry, true);
      updateStatus(entry.statusElement, `正在读取 ${entry.label} 账号列表…`, "info");
      let rawText;
      try {
        rawText = await clipboard.readText();
      } catch {
        setBusy(entry, false);
        (customOpenManualImport || openManualImportDialog)(section, entry);
        return false;
      }
      setBusy(entry, false);
      return importRawText(section, rawText);
    }

    sections.forEach((entry) => {
      entry.importButton?.addEventListener("click", () => {
        importSection(entry.section);
      });
      entry.exportButton?.addEventListener("click", () => {
        exportSection(entry.section);
      });
    });

    return { exportSection, importRawText, importSection };
  }

  globalThis.WebClxAccountClipboardManager = Object.freeze({
    create: createAccountClipboardManager,
  });
})();
