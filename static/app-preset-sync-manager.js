(function () {
  function createPresetSyncManager(deps) {
    const {
      state,
      requestJson,
      updateStatus,
      setDatalistOptions,
      refreshAuthPanels,
      loadProxyPresets,
      PRESET_SYNC_SECTIONS,
      PRESET_SYNC_CONFIG_ENDPOINT,
      PRESET_SYNC_PROXY_WARNING,
      elements,
    } = deps;
    const {
      presetSyncStatusEl,
      presetSyncPreviewBtnEl,
      presetSyncImportBtnEl,
      presetSyncRemoteUrlInputEl,
      presetSyncRemoteUrlHistoryEl,
      presetSyncAuthEnabledEl,
      presetSyncApiEnabledEl,
      presetSyncClaudeEnabledEl,
      presetSyncProxyEnabledEl,
      presetSyncAuthCountEl,
      presetSyncApiCountEl,
      presetSyncClaudeCountEl,
      presetSyncProxyCountEl,
      presetSyncProxyStateEl,
      settingsStatusEl,
    } = elements;

    function setPresetSyncBusy(busy) {
      if (presetSyncPreviewBtnEl) {
        presetSyncPreviewBtnEl.disabled = busy;
      }
      if (presetSyncImportBtnEl) {
        presetSyncImportBtnEl.disabled =
          busy || !state.remotePresetConfigSummary || readPresetSyncSelectedSections().length === 0;
      }
    }

    function formatPresetSyncToggle(enabled) {
      return enabled ? "开启" : "关闭";
    }

    function presetSyncSectionEntries() {
      return [
        { key: "auth_presets", label: "Codex_OAuth", input: presetSyncAuthEnabledEl },
        { key: "api_presets", label: "Codex_API", input: presetSyncApiEnabledEl },
        { key: "claude_presets", label: "Claude_API", input: presetSyncClaudeEnabledEl },
        { key: "proxy_presets", label: "代理预设/上游代理", input: presetSyncProxyEnabledEl },
      ];
    }

    function readPresetSyncSelectedSections() {
      const entries = presetSyncSectionEntries();
      const hasCheckboxes = entries.some((entry) => entry.input);
      if (!hasCheckboxes) {
        return PRESET_SYNC_SECTIONS.map((section) => section.key);
      }
      return entries.filter((entry) => entry.input?.checked).map((entry) => entry.key);
    }

    function presetSyncSelectedLabels(sections = readPresetSyncSelectedSections()) {
      const labelsByKey = new Map(PRESET_SYNC_SECTIONS.map((section) => [section.key, section.label]));
      return sections.map((section) => labelsByKey.get(section) || section);
    }

    function renderPresetSyncSummary(summary, sourceUrl = "") {
      state.remotePresetConfigSummary = summary || null;
      state.remotePresetConfigSourceUrl = sourceUrl || "";
      if (presetSyncAuthCountEl) {
        presetSyncAuthCountEl.textContent = summary ? String(summary.auth_preset_count || 0) : "-";
      }
      if (presetSyncApiCountEl) {
        presetSyncApiCountEl.textContent = summary ? String(summary.api_preset_count || 0) : "-";
      }
      if (presetSyncClaudeCountEl) {
        presetSyncClaudeCountEl.textContent = summary ? String(summary.claude_preset_count || 0) : "-";
      }
      if (presetSyncProxyCountEl) {
        presetSyncProxyCountEl.textContent = summary ? String(summary.proxy_preset_count || 0) : "-";
      }
      if (presetSyncProxyStateEl) {
        if (!summary) {
          presetSyncProxyStateEl.textContent = "未读取";
        } else {
          const lines = [
            `source_url=${sourceUrl || "-"}`,
            `codex_api_proxy=${formatPresetSyncToggle(summary.codex_api_proxy_enabled)}`,
            `claude_proxy=${formatPresetSyncToggle(summary.claude_proxy_enabled)}`,
            `active_proxy_id=${summary.active_proxy_id || "-"}`,
            `active_api_proxy_preset_id=${summary.active_api_proxy_preset_id || "-"}`,
            `active_claude_proxy_preset_id=${summary.active_claude_proxy_preset_id || "-"}`,
          ];
          presetSyncProxyStateEl.textContent = lines.join("\n");
        }
      }
      setPresetSyncBusy(false);
    }

    function normalizePresetSyncRemoteUrlValue(value) {
      const trimmed = typeof value === "string" ? value.trim() : "";
      if (!trimmed) {
        throw new Error("请输入远程 webClx 地址。");
      }
      const candidate = trimmed.includes("://") ? trimmed : `http://${trimmed}`;
      let url;
      try {
        url = new URL(candidate);
      } catch (error) {
        throw new Error(`远程 webClx 地址无效：${error.message}`);
      }
      if (!["http:", "https:"].includes(url.protocol) || !url.hostname) {
        throw new Error("远程 webClx 地址必须是有效的 http/https 地址。");
      }
      url.search = "";
      url.hash = "";
      const path = url.pathname.replace(/\/+$/, "");
      if (!path || path === "/") {
        url.pathname = "";
      } else if (path.endsWith(PRESET_SYNC_CONFIG_ENDPOINT)) {
        url.pathname = path.slice(0, -PRESET_SYNC_CONFIG_ENDPOINT.length) || "";
      } else {
        url.pathname = path;
      }
      return url.toString().replace(/\/$/, "");
    }

    function normalizePresetSyncRemoteUrlHistory(values) {
      const seen = new Set();
      const history = [];
      (Array.isArray(values) ? values : []).forEach((value) => {
        try {
          const normalized = normalizePresetSyncRemoteUrlValue(value);
          if (!seen.has(normalized)) {
            seen.add(normalized);
            history.push(normalized);
          }
        } catch (_error) {
          // Ignore stale invalid settings entries.
        }
      });
      return history.slice(0, 20);
    }

    function renderPresetSyncRemoteUrlHistory() {
      setDatalistOptions(presetSyncRemoteUrlHistoryEl, state.presetSyncRemoteUrlHistory);
    }

    function rememberPresetSyncRemoteUrl(value) {
      const normalized = normalizePresetSyncRemoteUrlValue(value);
      state.presetSyncRemoteUrlHistory = normalizePresetSyncRemoteUrlHistory([
        normalized,
        ...state.presetSyncRemoteUrlHistory,
      ]);
      renderPresetSyncRemoteUrlHistory();
      return normalized;
    }

    function readPresetSyncRemoteUrl() {
      try {
        const normalized = normalizePresetSyncRemoteUrlValue(presetSyncRemoteUrlInputEl?.value || "");
        if (presetSyncRemoteUrlInputEl) {
          presetSyncRemoteUrlInputEl.setCustomValidity("");
          presetSyncRemoteUrlInputEl.value = normalized;
        }
        return normalized;
      } catch (error) {
        if (presetSyncRemoteUrlInputEl) {
          presetSyncRemoteUrlInputEl.setCustomValidity(error.message);
          presetSyncRemoteUrlInputEl.reportValidity();
        }
        updateStatus(settingsStatusEl, error.message, "warn");
        return "";
      }
    }

    async function previewRemotePresetConfig() {
      const remoteUrl = readPresetSyncRemoteUrl();
      if (!remoteUrl) {
        updateStatus(presetSyncStatusEl, "请输入远程 webClx 地址。", "warn");
        return;
      }
      setPresetSyncBusy(true);
      updateStatus(presetSyncStatusEl, "正在读取远程预设配置…", "info");
      try {
        const response = await requestJson("/api/settings/preset-config/remote-preview", {
          method: "POST",
          body: JSON.stringify({ remote_url: remoteUrl }),
        });
        rememberPresetSyncRemoteUrl(response.source_url || remoteUrl);
        renderPresetSyncSummary(response.summary, response.source_url);
        updateStatus(presetSyncStatusEl, "远程预设已读取，确认数量后可导入。", "ok");
      } catch (error) {
        renderPresetSyncSummary(null);
        updateStatus(presetSyncStatusEl, error.message || "读取远程预设失败。", "warn");
      } finally {
        setPresetSyncBusy(false);
      }
    }

    async function importRemotePresetConfig() {
      const remoteUrl = readPresetSyncRemoteUrl();
      if (!remoteUrl) {
        updateStatus(presetSyncStatusEl, "请输入远程 webClx 地址。", "warn");
        return;
      }
      const sections = readPresetSyncSelectedSections();
      if (sections.length === 0) {
        updateStatus(presetSyncStatusEl, "请至少选择一个要覆盖的配置类别。", "warn");
        setPresetSyncBusy(false);
        return;
      }
      const selectedLabels = presetSyncSelectedLabels(sections);
      const includesProxyPresets = sections.includes("proxy_presets");
      const warningText = includesProxyPresets ? `\n\n${PRESET_SYNC_PROXY_WARNING}` : "";
      const confirmed = window.confirm(
        `确定导入远程选中的配置吗？\n\n将覆盖本机：${selectedLabels.join("、")}。${warningText}`,
      );
      if (!confirmed) {
        return;
      }
      setPresetSyncBusy(true);
      updateStatus(presetSyncStatusEl, `正在导入：${selectedLabels.join("、")}…`, "info");
      try {
        const response = await requestJson("/api/settings/preset-config/import-remote", {
          method: "POST",
          body: JSON.stringify({
            remote_url: remoteUrl,
            sections,
            confirm_proxy_presets: includesProxyPresets,
          }),
        });
        rememberPresetSyncRemoteUrl(response.source_url || remoteUrl);
        renderPresetSyncSummary(response.summary, response.source_url);
        const selectedSet = new Set(sections);
        if (selectedSet.has("auth_presets")) {
          state.authPresetsLoaded = false;
        }
        if (selectedSet.has("api_presets")) {
          state.apiPresetsLoaded = false;
        }
        if (selectedSet.has("claude_presets")) {
          state.claudePresetsLoaded = false;
        }
        const activeAuthTabUpdated =
          (state.activeTab === "auth" && selectedSet.has("auth_presets")) ||
          (state.activeTab === "api" && selectedSet.has("api_presets")) ||
          (state.activeTab === "claude" && selectedSet.has("claude_presets"));
        if (activeAuthTabUpdated) {
          await refreshAuthPanels();
        }
        if (selectedSet.has("proxy_presets") && state.activeSettingsTab === "proxy") {
          await loadProxyPresets();
        } else if (selectedSet.has("proxy_presets")) {
          state.proxyPresets = [];
        }
        updateStatus(presetSyncStatusEl, `远程配置已导入：${selectedLabels.join("、")}。`, "ok");
        updateStatus(settingsStatusEl, `预设配置已从远程更新：${selectedLabels.join("、")}。`, "info");
      } catch (error) {
        updateStatus(presetSyncStatusEl, error.message || "导入远程预设失败。", "warn");
      } finally {
        setPresetSyncBusy(false);
      }
    }

    if (presetSyncPreviewBtnEl) {
      presetSyncPreviewBtnEl.addEventListener("click", () => {
        previewRemotePresetConfig();
      });
    }

    if (presetSyncImportBtnEl) {
      presetSyncImportBtnEl.addEventListener("click", () => {
        importRemotePresetConfig();
      });
    }

    if (presetSyncRemoteUrlInputEl) {
      presetSyncRemoteUrlInputEl.addEventListener("input", () => {
        presetSyncRemoteUrlInputEl.setCustomValidity("");
      });
    }

    presetSyncSectionEntries().forEach((entry) => {
      entry.input?.addEventListener("change", () => {
        setPresetSyncBusy(false);
        if (entry.key === "proxy_presets" && entry.input.checked) {
          updateStatus(presetSyncStatusEl, PRESET_SYNC_PROXY_WARNING, "warn");
          return;
        }
        if (state.remotePresetConfigSummary && readPresetSyncSelectedSections().length === 0) {
          updateStatus(presetSyncStatusEl, "请至少选择一个要覆盖的配置类别。", "warn");
        }
      });
    });

    return {
      importRemotePresetConfig,
      normalizePresetSyncRemoteUrlHistory,
      previewRemotePresetConfig,
      renderPresetSyncRemoteUrlHistory,
      setPresetSyncBusy,
    };
  }

  globalThis.WebClxPresetSyncManager = Object.freeze({ create: createPresetSyncManager });
})();
