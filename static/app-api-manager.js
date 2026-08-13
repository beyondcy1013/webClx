(function () {
  function createApiManager(deps) {
    const {
      state,
      requestJson,
      updateStatus,
      setInlineStatus,
      setButtonBusy,
      refreshAuthPanels,
      normalizeUpstreamProxySettings,
      renderUpstreamProxyToggles,
      refreshBaseUrlPresetOptions,
      refreshConfigOverrideDatalists,
      renderConfigOverrideEditor,
      collectConfigOverrideEditorValues,
      normalizePresetConfigOverrides,
      collectPresetConfigKeys,
      normalizePresetBaseUrlGroupKey,
      renderApiPresetTableHeader,
      renderPresetTable,
      movePresetById,
      movePresetOrderWithPersist,
      persistPresetOrder,
      buildPresetConfigValueMap,
      buildPresetConfigCells,
      createActionCell,
      createActionButton,
      createPresetActionMenu,
      createCurrentIndicatorCell,
      createPresetNameCell,
      createTextCell,
      createExternalUrlCell,
      formatDateTime,
      formatTerminalStartupEnvVars,
      parseTerminalStartupEnvInput,
      normalizeTerminalStartupEnvVars,
      apiBaseUrlMatchesSelectedAutoProxyProvider,
      confirmEditedPresetOverwrite,
      formatCurrentApiHeadline,
      normalizeTestResult,
      errorTestResult,
      prunePresetTestResults,
      presetTestPopup,
      API_FORM_DEFAULT_STATUS,
      importAuthAccounts,
      importAuthAccountFiles,
      elements,
    } = deps;
    const {
      apiManagerStatusEl,
      apiCurrentFileEl,
      apiConfigFileEl,
      apiCurrentTargetEl,
      apiPresetFileEl,
      apiPresetGroupModeInputEl,
      apiPresetSearchInputEl,
      apiPresetSelectionModeButtonEl,
      apiPresetNameEl,
      apiPresetRowNumberEl,
      apiConfigOverrideControls,
      apiKeyInputEl,
      apiBaseUrlInputEl,
      apiWireApiInputEl,
      apiResponsesProxyInputEl,
      apiModelInputEl,
      apiModelPresetsEl,
      apiManagementUrlInputEl,
      apiManagementUrlPanelEl,
      apiManagementUrlSameAsBaseInputEl,
      apiApplyUpstreamProxyOnSwitchInputEl,
      apiTerminalStartupDetailsEl,
      apiTerminalEnvInputEl,
      apiTerminalStartupScriptInputEl,
      apiPresetEditorPanelEl,
      apiSavePresetButton,
      apiSaveAsNewPresetButton,
      apiApplyEditedPresetButton,
      apiClearInputButton,
      apiTestAllPresetsButton,
      apiClipboardExportButton,
      apiFormStatusEl,
      apiPresetListEl,
      apiPresetMobileListEl,
      apiAccountImportDialogEl,
      apiAccountImportTextEl,
      apiAccountImportFileButton,
      apiAccountImportTextButton,
      apiAccountImportSubmitButton,
      apiAccountImportStatusEl,
    } = elements;

    function apiPresetHasTerminalStartupSettings(preset) {
      return (
        normalizeTerminalStartupEnvVars(preset?.terminal_env).length > 0 ||
        Boolean(String(preset?.terminal_startup_script || "").trim())
      );
    }

    function currentApiApplyProxyRecommendation() {
      const responsesProxy = apiResponsesProxyInputEl?.value || "direct";
      return responsesProxy !== "direct"
        || apiBaseUrlMatchesSelectedAutoProxyProvider(apiBaseUrlInputEl?.value || "");
    }

    function apiApplyProxyRecommendationWarningMessage() {
      return "当前 API 预设需要使用 webClx 本机入口或转换模式；不启用本机入口可能无法正常对话。保存时会自动勾选，手动取消后保存会再次确认。";
    }

    function warnApiApplyProxyRecommendationIfNeeded() {
      if (!apiApplyUpstreamProxyOnSwitchInputEl) {
        return false;
      }

      const recommended = currentApiApplyProxyRecommendation();
      if (recommended && !apiApplyUpstreamProxyOnSwitchInputEl.checked) {
        updateStatus(apiFormStatusEl, apiApplyProxyRecommendationWarningMessage(), "warn");
        return true;
      }
      return false;
    }

    function syncApiApplyProxyRecommendation() {
      if (!apiApplyUpstreamProxyOnSwitchInputEl) {
        return false;
      }

      const recommended = currentApiApplyProxyRecommendation();
      if (!recommended) {
        state.apiApplyProxyManuallyChanged = false;
      }
      if (recommended && !state.apiApplyProxyManuallyChanged) {
        apiApplyUpstreamProxyOnSwitchInputEl.checked = true;
      }
      apiApplyUpstreamProxyOnSwitchInputEl.title = recommended
        ? "当前 Base URL 命中设置页模型 TAB 的自动匹配项。"
        : "";
      warnApiApplyProxyRecommendationIfNeeded();
      return recommended;
    }

    function confirmApiApplyProxyRecommendationBeforeSave() {
      if (!warnApiApplyProxyRecommendationIfNeeded()) {
        return true;
      }
      return window.confirm(
        `${apiApplyProxyRecommendationWarningMessage()}\n\n仍然不勾选并继续保存吗？`,
      );
    }

    function readApiApplyUpstreamProxyOnSwitch() {
      return Boolean(apiApplyUpstreamProxyOnSwitchInputEl?.checked);
    }

    function formatApiResponsesProxyMode(value) {
      switch (value) {
        case "direct":
          return "不转换";
        case "openai_chat":
          return "OpenAI Chat → Responses";
        case "deepseek_chat":
          return "DeepSeek Chat → Responses";
        case "minimax_chat":
          return "MiniMax Chat → Responses";
        case "anthropic_chat":
          return "Anthropic Messages → Responses";
        default:
          return "不转换";
      }
    }

    // The `model` config key is promoted to a dedicated top-level field so each
    // API preset self-carries the model that matches its provider. Switching a
    // preset therefore updates model and provider atomically, instead of
    // leaving the previous model stale in config.toml. The generic config
    // override editor only holds non-model overrides.
    const API_MODEL_CONFIG_KEY = "model";
    const API_PRESET_GROUP_MODE_STORAGE_KEY = "webclx:api-preset-group-mode";
    const expandedApiPresetIds = new Set();
    let apiPresetMutationRevision = 0;
    let apiPresetReloadPending = false;

    function isModelConfigOverride(item) {
      return normalizeConfigOverrideValue(item?.key).trim().toLowerCase() === API_MODEL_CONFIG_KEY;
    }

    function extractModelFromOverrides(overrides) {
      const list = Array.isArray(overrides) ? overrides : [];
      for (let index = list.length - 1; index >= 0; index -= 1) {
        if (isModelConfigOverride(list[index])) {
          return normalizeConfigOverrideValue(list[index]?.value).trim();
        }
      }
      return "";
    }

    function apiPresetModel(preset) {
      return extractModelFromOverrides(preset?.config_overrides);
    }

    function normalizeApiPresetGroupMode(value) {
      return value === "model" ? "model" : "base_url";
    }

    function normalizeApiPresetModelGroupKey(value) {
      return String(value || "").trim().toLowerCase();
    }

    function apiPresetGroupConfig() {
      if (state.apiPresetGroupMode === "model") {
        return {
          getKey: apiPresetModel,
          normalizeKey: normalizeApiPresetModelGroupKey,
          mergeCellKey: "config:model",
        };
      }
      return {
        getKey: (preset) => preset?.base_url || "",
        normalizeKey: normalizePresetBaseUrlGroupKey,
        mergeCellKey: "base_url",
      };
    }

    function initializeApiPresetGroupMode() {
      if (!state) {
        return;
      }
      let savedMode = "";
      try {
        savedMode = window.localStorage.getItem(API_PRESET_GROUP_MODE_STORAGE_KEY) || "";
      } catch {}
      state.apiPresetGroupMode = normalizeApiPresetGroupMode(savedMode);
      if (!apiPresetGroupModeInputEl) {
        return;
      }
      apiPresetGroupModeInputEl.value = state.apiPresetGroupMode;
      apiPresetGroupModeInputEl.addEventListener("change", () => {
        state.apiPresetGroupMode = normalizeApiPresetGroupMode(apiPresetGroupModeInputEl.value);
        apiPresetGroupModeInputEl.value = state.apiPresetGroupMode;
        try {
          window.localStorage.setItem(API_PRESET_GROUP_MODE_STORAGE_KEY, state.apiPresetGroupMode);
        } catch {}
        state.presetTableSort?.delete?.("api");
        renderApiPresets(state.apiPresets);
      });
    }

    function apiPresetMatchesSearch(preset, searchTerm) {
      if (!searchTerm) return true;
      const searchableText = [
        preset?.name,
        preset?.provider_name,
        preset?.base_url,
        preset?.management_url,
        apiPresetModel(preset),
      ]
        .filter(Boolean)
        .join("\n")
        .toLocaleLowerCase();
      return searchableText.includes(searchTerm);
    }

    function filteredApiPresets(presets) {
      const searchTerm = String(state.apiPresetSearchTerm || "").trim().toLocaleLowerCase();
      return (Array.isArray(presets) ? presets : []).filter(
        (preset) => apiPresetMatchesSearch(preset, searchTerm),
      );
    }

    function updateApiPresetSelectionControls() {
      const count = state.apiPresetExportSelection.size;
      if (apiPresetSelectionModeButtonEl) {
        apiPresetSelectionModeButtonEl.setAttribute(
          "aria-pressed",
          state.apiPresetSelectionMode ? "true" : "false",
        );
        apiPresetSelectionModeButtonEl.textContent = state.apiPresetSelectionMode
          ? (count > 0 ? `已选 ${count}` : "取消选择")
          : "选择";
        apiPresetSelectionModeButtonEl.title = state.apiPresetSelectionMode
          ? "退出选择模式"
          : "选择要批量导出的预设";
      }
      if (apiClipboardExportButton) {
        apiClipboardExportButton.disabled = count === 0;
      }
    }

    function initializeApiPresetListControls() {
      if (!state) return;
      if (apiPresetSearchInputEl) {
        apiPresetSearchInputEl.value = state.apiPresetSearchTerm || "";
        apiPresetSearchInputEl.addEventListener("input", () => {
          state.apiPresetSearchTerm = apiPresetSearchInputEl.value;
          renderApiPresets(state.apiPresets);
        });
      }
      if (apiPresetSelectionModeButtonEl) {
        apiPresetSelectionModeButtonEl.addEventListener("click", () => {
          state.apiPresetSelectionMode = !state.apiPresetSelectionMode;
          if (!state.apiPresetSelectionMode) state.apiPresetExportSelection.clear();
          renderApiPresets(state.apiPresets);
        });
      }
      updateApiPresetSelectionControls();
    }

    // Overrides shown in the generic editor (everything except `model`, which
    // lives in its own field).
    function overridesWithoutModel(overrides) {
      const list = Array.isArray(overrides) ? overrides : [];
      return list.filter((item) => !isModelConfigOverride(item));
    }

    // Merge the model field back into the override list for persistence. The
    // model field is authoritative; any stray `model` row left in the generic
    // editor is dropped to avoid duplicates.
    function mergeModelIntoOverrides(overrides) {
      const modelValue = (apiModelInputEl?.value || "").trim();
      const others = (Array.isArray(overrides) ? overrides : []).filter(
        (item) => !isModelConfigOverride(item),
      );
      if (!modelValue) {
        return others;
      }
      return [{ key: API_MODEL_CONFIG_KEY, value: modelValue }, ...others];
    }

    function apiStatusSortRank(preset) {
      if (preset?.active) {
        return 3;
      }
      if (state.apiPresetsTesting.has(preset?.id)) {
        return 2;
      }
      const result = state.apiPresetTestResults.get(preset?.id);
      if (result) {
        return result.ok ? 1 : -1;
      }
      return 0;
    }

    function buildApiPresetSortColumns(configKeys) {
      return [
        { key: "status", type: "number", defaultDirection: "desc", getValue: apiStatusSortRank },
        { key: "name", type: "text", getValue: (preset) => preset?.name || "" },
        { key: "base_url", type: "text", getValue: (preset) => preset?.base_url || "" },
        { key: "wire_api", type: "text", getValue: (preset) => preset?.wire_api || "responses" },
        { key: "responses_proxy", type: "text", getValue: (preset) => formatApiResponsesProxyMode(preset?.responses_proxy) },
        { key: "local_proxy", type: "boolean", defaultDirection: "desc", getValue: (preset) => preset?.apply_upstream_proxy_on_switch },
        { key: "management_url", type: "text", getValue: (preset) => preset?.management_url || "" },
        {
          key: "api_key",
          type: "text",
          getValue: (preset) => preset?.access_mode === "chatgpt_oauth"
            ? preset?.masked_access_token || ""
            : preset?.masked_api_key || "",
        },
        { key: "terminal_env", type: "text", getValue: (preset) => formatTerminalStartupEnvVars(preset?.terminal_env) || "" },
        { key: "terminal_script", type: "boolean", defaultDirection: "desc", getValue: (preset) => Boolean(preset?.terminal_startup_script) },
        { key: "switch_count", type: "number", defaultDirection: "desc", getValue: (preset) => preset?.switch_count || 0 },
        ...createPresetConfigSortColumns(configKeys, "config.toml: "),
        { key: "saved_at", type: "date", defaultDirection: "desc", getValue: (preset) => preset?.saved_at || "" },
      ];
    }

    function apiPresetGroupKey(preset, index = 0) {
      const group = apiPresetGroupConfig();
      const normalized = group.normalizeKey(group.getKey(preset), preset);
      return normalized || `__ungrouped:${index}`;
    }

    function apiPresetVisibleOrder(presets = state.apiPresets) {
      const groups = [];
      const groupsByKey = new Map();
      (Array.isArray(presets) ? presets : []).forEach((preset, index) => {
        const groupKey = apiPresetGroupKey(preset, index);
        let groupRecord = groupsByKey.get(groupKey);
        if (!groupRecord) {
          groupRecord = { key: groupKey, items: [] };
          groupsByKey.set(groupKey, groupRecord);
          groups.push(groupRecord);
        }
        groupRecord.items.push(preset);
      });
      return groups.flatMap((group) => group.items);
    }

    function apiPresetVisibleRowNumber(presetId, presets = state.apiPresets) {
      const index = apiPresetVisibleOrder(presets).findIndex((preset) => preset?.id === presetId);
      return index >= 0 ? index + 1 : 0;
    }

    function moveApiPresetToVisibleRow(presetId, rowNumber, presets = state.apiPresets) {
      const rows = Array.isArray(presets) ? presets.slice() : [];
      const fromIndex = rows.findIndex((preset) => preset?.id === presetId);
      if (fromIndex < 0) {
        return null;
      }

      const normalizedRowNumber = Math.max(1, Math.min(rows.length, Math.trunc(Number(rowNumber) || 1)));
      const movingPreset = rows[fromIndex];
      rows.splice(fromIndex, 1);
      const visibleWithoutMoving = apiPresetVisibleOrder(rows);
      const targetVisibleBefore = visibleWithoutMoving[normalizedRowNumber - 1] || null;
      let insertIndex = targetVisibleBefore
        ? rows.findIndex((preset) => preset?.id === targetVisibleBefore.id)
        : rows.length;
      if (insertIndex < 0) {
        insertIndex = rows.length;
      }
      rows.splice(insertIndex, 0, movingPreset);
      return rows;
    }

    async function moveApiPresetOrder(presetId, direction) {
      const step = direction < 0 ? -1 : 1;
      const visiblePresets = apiPresetVisibleOrder(state.apiPresets);
      const visibleIndex = visiblePresets.findIndex((preset) => preset?.id === presetId);
      const targetPreset = visiblePresets[visibleIndex + step];
      const movingPreset = visiblePresets[visibleIndex];
      if (
        !movingPreset ||
        !targetPreset ||
        apiPresetGroupKey(movingPreset, visibleIndex) !==
          apiPresetGroupKey(targetPreset, visibleIndex + step)
      ) {
        return false;
      }

      const nextPresets = state.apiPresets.slice();
      const fromIndex = nextPresets.findIndex((preset) => preset?.id === movingPreset.id);
      const targetIndex = nextPresets.findIndex((preset) => preset?.id === targetPreset.id);
      if (fromIndex < 0 || targetIndex < 0) {
        return false;
      }
      [nextPresets[fromIndex], nextPresets[targetIndex]] = [
        nextPresets[targetIndex],
        nextPresets[fromIndex],
      ];

      const previousPresets = state.apiPresets;
      state.apiPresets = nextPresets;
      state.presetTableSort?.delete?.("api");
      renderApiPresets(state.apiPresets);
      try {
        await persistPresetOrder("/api/auth/api-presets/reorder", state.apiPresets);
        setInlineStatus(apiManagerStatusEl, "API 预设顺序已保存。", "success");
        return true;
      } catch (error) {
        state.apiPresets = previousPresets;
        renderApiPresets(state.apiPresets);
        throw error;
      }
    }

    async function moveApiPresetToRowNumber(presetId, rowNumber) {
      const targetRowNumber = Number(rowNumber);
      if (!Number.isFinite(targetRowNumber)) {
        return false;
      }
      const nextPresets = moveApiPresetToVisibleRow(
        presetId,
        targetRowNumber,
        state.apiPresets,
      );
      if (!nextPresets) {
        return false;
      }
      const previousPresets = state.apiPresets;
      state.apiPresets = nextPresets;
      state.presetTableSort?.delete?.("api");
      renderApiPresets(state.apiPresets);

      try {
        await persistPresetOrder("/api/auth/api-presets/reorder", state.apiPresets);
        setInlineStatus(apiManagerStatusEl, "API 预设顺序已保存。", "success");
        return true;
      } catch (error) {
        state.apiPresets = previousPresets;
        renderApiPresets(state.apiPresets);
        throw error;
      }
    }

    function normalizeApiPresetExternalUrl(value) {
      const normalized = String(value || "").trim();
      if (!normalized) return "";
      try {
        const url = new URL(normalized);
        return url.protocol === "http:" || url.protocol === "https:" ? url.href : "";
      } catch {
        return "";
      }
    }

    function apiPresetRowActions(preset) {
      const visiblePresets = apiPresetVisibleOrder(state.apiPresets);
      const index = visiblePresets.findIndex((item) => item?.id === preset?.id);
      const groupKey = index >= 0 ? apiPresetGroupKey(preset, index) : "";
      const previousPreset = index > 0 ? visiblePresets[index - 1] : null;
      const nextPreset = index >= 0 ? visiblePresets[index + 1] : null;
      const canMoveUp = Boolean(
        previousPreset && apiPresetGroupKey(previousPreset, index - 1) === groupKey,
      );
      const canMoveDown = Boolean(
        nextPreset && apiPresetGroupKey(nextPreset, index + 1) === groupKey,
      );
      const actions = [
        {
          label: "切换并启动",
          handler: () => applyApiPresetAndLaunch(preset.id),
        },
        {
          label: "测试",
          handler: () => testApiPreset(preset.id, preset.name),
        },
        {
          label: "编辑",
          handler: () => editApiPreset(preset.id),
        },
      ];
      const managementUrl = normalizeApiPresetExternalUrl(preset.management_url);
      if (managementUrl) {
        actions.push({ label: "打开管理页面", href: managementUrl });
      }
      actions.push(
        {
          label: "上移",
          disabled: !canMoveUp,
          handler: () => moveApiPresetOrder(preset.id, -1),
        },
        {
          label: "下移",
          disabled: !canMoveDown,
          handler: () => moveApiPresetOrder(preset.id, 1),
        },
        {
          label: "删除",
          danger: true,
          handler: () => deleteApiPreset(preset.id, preset.name),
        },
      );
      return actions;
    }

    function apiPresetDisplayHost(value) {
      const normalized = String(value || "").trim();
      if (!normalized) return "未设置 URL";
      try {
        return new URL(normalized).host || normalized;
      } catch {
        return normalized;
      }
    }

    function createApiPresetMobileStatus(preset) {
      const wrap = document.createElement("div");
      wrap.className = "api-mobile-preset-status";
      if (preset.active) {
        const current = document.createElement("span");
        current.className = "api-mobile-current-badge";
        current.textContent = "当前";
        wrap.appendChild(current);
      }
      const indicatorCell = createCurrentIndicatorCell(preset.active, {
        testResult: state.apiPresetTestResults.get(preset.id) || null,
        testKind: "api",
        testing: state.apiPresetsTesting.has(preset.id),
      });
      indicatorCell.querySelector(".current-indicator-arrow")?.remove();
      wrap.append(...indicatorCell.childNodes);
      if (!wrap.childNodes.length) {
        const idle = document.createElement("span");
        idle.className = "api-mobile-untested-label";
        idle.textContent = "未测试";
        wrap.appendChild(idle);
      }
      return wrap;
    }

    function appendApiPresetMobileDetail(list, label, value, { mono = false } = {}) {
      const term = document.createElement("dt");
      term.textContent = label;
      const detail = document.createElement("dd");
      if (mono) detail.classList.add("mono-text");
      if (value instanceof Node) {
        detail.appendChild(value);
      } else {
        detail.textContent = String(value || "—");
      }
      list.append(term, detail);
    }

    function createApiPresetMobileDetails(preset, configKeys, detailsId) {
      const details = document.createElement("div");
      details.id = detailsId;
      details.className = "api-mobile-preset-details";
      details.hidden = !expandedApiPresetIds.has(preset.id);
      const list = document.createElement("dl");
      list.className = "api-mobile-preset-details-grid";
      appendApiPresetMobileDetail(list, "Base URL", preset.base_url, { mono: true });
      appendApiPresetMobileDetail(list, "模型", apiPresetModel(preset) || "未设置", { mono: true });
      appendApiPresetMobileDetail(list, "Wire", preset.wire_api || "responses", { mono: true });
      appendApiPresetMobileDetail(
        list,
        "协议转换",
        formatApiResponsesProxyMode(preset.responses_proxy),
      );
      appendApiPresetMobileDetail(
        list,
        "本机入口",
        preset.access_mode === "chatgpt_oauth"
          ? "OAuth 代理"
          : preset.apply_upstream_proxy_on_switch ? "是" : "否",
      );
      const managementUrl = normalizeApiPresetExternalUrl(preset.management_url);
      if (managementUrl) {
        const link = document.createElement("a");
        link.href = managementUrl;
        link.target = "_blank";
        link.rel = "noopener noreferrer";
        link.textContent = apiPresetDisplayHost(managementUrl);
        appendApiPresetMobileDetail(list, "管理页面", link);
      }
      appendApiPresetMobileDetail(
        list,
        "凭据",
        preset.access_mode === "chatgpt_oauth"
          ? `OAuth Token ${preset.masked_access_token || "已保存"}`
          : preset.masked_api_key,
        { mono: true },
      );
      appendApiPresetMobileDetail(
        list,
        "启动环境",
        formatTerminalStartupEnvVars(preset.terminal_env) || "无",
        { mono: true },
      );
      appendApiPresetMobileDetail(
        list,
        "启动脚本",
        preset.terminal_startup_script ? "已设置" : "无",
      );
      appendApiPresetMobileDetail(list, "使用次数", String(preset.switch_count || 0));
      const configValues = buildPresetConfigValueMap(preset);
      const configSummary = configKeys
        .map((key) => `${key}=${configValues.get(key) || "—"}`)
        .join(" · ");
      appendApiPresetMobileDetail(list, "config.toml", configSummary || "无", { mono: true });
      appendApiPresetMobileDetail(list, "保存时间", formatDateTime(preset.saved_at));
      details.appendChild(list);
      return details;
    }

    function createApiPresetMobileRow(preset, configKeys, actions) {
      const row = document.createElement("div");
      row.className = "api-mobile-preset-row";
      row.setAttribute("role", "listitem");
      if (preset.active) row.classList.add("active-auth-row");
      if (state.apiPresetExportSelection.has(preset.id)) row.classList.add("is-selected");

      const summary = document.createElement("div");
      summary.className = "api-mobile-preset-summary";
      if (state.apiPresetSelectionMode) {
        summary.classList.add("has-selection");
        const checkbox = document.createElement("input");
        checkbox.type = "checkbox";
        checkbox.className = "api-mobile-preset-checkbox";
        checkbox.checked = state.apiPresetExportSelection.has(preset.id);
        checkbox.setAttribute("aria-label", `选择 ${preset.name || preset.id}`);
        checkbox.addEventListener("change", () => {
          if (checkbox.checked) state.apiPresetExportSelection.add(preset.id);
          else state.apiPresetExportSelection.delete(preset.id);
          renderApiPresets(state.apiPresets);
        });
        summary.appendChild(checkbox);
      }

      const detailsId = `api-mobile-preset-${String(preset.id).replace(/[^a-zA-Z0-9_-]/g, "-")}`;
      const identityButton = document.createElement("button");
      identityButton.type = "button";
      identityButton.className = "api-mobile-preset-identity";
      identityButton.setAttribute("aria-controls", detailsId);
      identityButton.setAttribute("aria-expanded", expandedApiPresetIds.has(preset.id) ? "true" : "false");
      const name = document.createElement("strong");
      name.className = "api-mobile-preset-name";
      name.textContent = preset.name || "未命名预设";
      const meta = document.createElement("span");
      meta.className = "api-mobile-preset-meta mono-text";
      meta.textContent = [apiPresetModel(preset) || "未设置模型", apiPresetDisplayHost(preset.base_url)]
        .join(" · ");
      identityButton.append(name, meta, createApiPresetMobileStatus(preset));

      const primaryButton = createActionButton(
        "切换",
        () => applyApiPreset(preset.id),
        "mini-button accent api-mobile-preset-primary",
      );
      const menuButton = createPresetActionMenu(actions, {
        label: `${preset.name || "当前预设"} 的更多操作`,
      });
      summary.append(identityButton, primaryButton, menuButton);

      const details = createApiPresetMobileDetails(preset, configKeys, detailsId);
      identityButton.addEventListener("click", () => {
        const expanded = !expandedApiPresetIds.has(preset.id);
        if (expanded) expandedApiPresetIds.add(preset.id);
        else expandedApiPresetIds.delete(preset.id);
        identityButton.setAttribute("aria-expanded", expanded ? "true" : "false");
        details.hidden = !expanded;
      });
      row.append(summary, details);
      return row;
    }

    function renderApiPresetMobileList(presets, configKeys) {
      if (!apiPresetMobileListEl) return;
      apiPresetMobileListEl.replaceChildren();
      if (!presets.length) {
        const empty = document.createElement("p");
        empty.className = "meta-text api-mobile-preset-empty";
        empty.textContent = state.apiPresetSearchTerm
          ? "没有匹配的 API 预设。"
          : "还没有保存任何 API 预设。";
        apiPresetMobileListEl.appendChild(empty);
        return;
      }
      let previousGroupKey = null;
      presets.forEach((preset, index) => {
        const groupKey = apiPresetGroupKey(preset, index);
        if (groupKey !== previousGroupKey) {
          const groupLabel = document.createElement("div");
          groupLabel.className = "api-mobile-preset-group-label";
          groupLabel.textContent = state.apiPresetGroupMode === "model"
            ? apiPresetModel(preset) || "未设置模型"
            : preset.base_url || "未设置 Base URL";
          apiPresetMobileListEl.appendChild(groupLabel);
          previousGroupKey = groupKey;
        }
        apiPresetMobileListEl.appendChild(
          createApiPresetMobileRow(preset, configKeys, apiPresetRowActions(preset)),
        );
      });
    }

    function renderApiPresets(presets) {
      const configKeys = collectPresetConfigKeys(presets);
      const groupConfig = apiPresetGroupConfig();
      const filteredPresets = filteredApiPresets(presets);
      const visiblePresets = apiPresetVisibleOrder(filteredPresets);
      const sortColumns = buildApiPresetSortColumns(configKeys);
      const validIds = new Set(presets.map((preset) => preset.id));
      state.apiPresetExportSelection.forEach((id) => {
        if (!validIds.has(id)) state.apiPresetExportSelection.delete(id);
      });
      expandedApiPresetIds.forEach((id) => {
        if (!validIds.has(id)) expandedApiPresetIds.delete(id);
      });
      const selection = state.apiPresetSelectionMode
        ? {
            label: "Codex_API 账号",
            presets: visiblePresets,
            selectedIds: state.apiPresetExportSelection,
            onChange: () => renderApiPresets(presets),
          }
        : null;
      updateApiPresetSelectionControls();
      renderApiPresetTableHeader(configKeys, {
        sortColumns,
        onSortChange: () => renderApiPresets(presets),
        selection,
      });

      renderPresetTable({
        listEl: apiPresetListEl,
        presets: visiblePresets,
        emptyText: state.apiPresetSearchTerm
          ? "没有匹配的 API 预设。"
          : "还没有保存任何 API 预设。",
        group: groupConfig,
        emptyColspan: 14 + configKeys.length,
        tableKey: "api",
        sortColumns,
        selection,
        buildCells: (preset) => {
          const currentCell = createCurrentIndicatorCell(preset.active, {
            testResult: state.apiPresetTestResults.get(preset.id) || null,
            testKind: "api",
            testing: state.apiPresetsTesting.has(preset.id),
          });
          currentCell.classList.add("api-preset-status-cell");
          const nameCell = createPresetNameCell(preset.name);
          nameCell.classList.add("api-preset-name-cell");
          const baseUrlCell = createTextCell(preset.base_url, "mono-text");
          baseUrlCell.classList.add("api-preset-base-url-cell");
          baseUrlCell.dataset.presetColumn = "base_url";
          const managementUrlCell = createExternalUrlCell(preset.management_url);
          const wireApiCell = createTextCell(preset.wire_api || "responses", "mono-text");

          const responsesProxyCell = createTextCell(
            formatApiResponsesProxyMode(preset.responses_proxy),
            "mono-text",
          );
          if (preset.provider_base_url) {
            responsesProxyCell.title = `Provider Base URL: ${preset.provider_base_url}`;
          }
          const isChatgptOauth = preset.access_mode === "chatgpt_oauth";
          const applyProxyCell = createTextCell(
            isChatgptOauth
              ? "OAuth 代理"
              : preset.apply_upstream_proxy_on_switch ? "是" : "否",
            "mono-text",
          );

          const keyCell = createTextCell(
            isChatgptOauth
              ? `OAuth Token ${preset.masked_access_token || "已保存"}`
              : preset.masked_api_key,
            "mono-text",
          );
          if (isChatgptOauth && preset.account_id) {
            keyCell.title = `ChatGPT Account ID: ${preset.account_id}`;
          }
          const terminalEnvCell = createTextCell(
            formatTerminalStartupEnvVars(preset.terminal_env) || "无",
            "mono-text",
          );
          const terminalScriptCell = createTextCell(
            preset.terminal_startup_script ? "已设置" : "无",
            "mono-text",
          );
          if (preset.terminal_startup_script) {
            terminalScriptCell.title = preset.terminal_startup_script;
          }
          const switchCountCell = createTextCell(String(preset.switch_count || 0), "mono-text");

          const configValues = buildPresetConfigValueMap(preset);
          const configCells = buildPresetConfigCells(configKeys, configValues);
          configCells.forEach((cell, index) => {
            cell.dataset.presetColumn = `config:${configKeys[index]}`;
          });
          const timeCell = createTextCell(formatDateTime(preset.saved_at));
          const actionCell = createActionCell(
            [
              createActionButton(
                "切换",
                () => applyApiPreset(preset.id),
                "mini-button accent",
              ),
              createPresetActionMenu(apiPresetRowActions(preset), {
                label: `${preset.name || "当前预设"} 的更多操作`,
              }),
            ],
            "auth-action-cell api-preset-operation-cell",
            "actions preset-actions api-preset-consolidated-actions",
          );

          return [
            actionCell,
            currentCell,
            nameCell,
            baseUrlCell,
            wireApiCell,
            responsesProxyCell,
            applyProxyCell,
            managementUrlCell,
            keyCell,
            terminalEnvCell,
            terminalScriptCell,
            switchCountCell,
            ...configCells,
            timeCell,
          ];
        },
      });
      renderApiPresetMobileList(visiblePresets, configKeys);

      if (apiModelPresetsEl) {
        const seen = new Set();
        const models = [];
        for (const preset of presets) {
          const model = extractModelFromOverrides(
            Array.isArray(preset?.config_overrides) ? preset.config_overrides : [],
          );
          if (model && !seen.has(model.toLowerCase())) {
            seen.add(model.toLowerCase());
            models.push(model);
          }
        }
        setDatalistOptions(apiModelPresetsEl, models);
      }
    }

    async function loadApiPresets() {
      if (state.apiPresetsLoading) {
        apiPresetReloadPending = true;
        return;
      }

      const requestRevision = apiPresetMutationRevision;
      state.apiPresetsLoading = true;
      setInlineStatus(apiManagerStatusEl, "", "info", true);
      if (apiTestAllPresetsButton) {
        apiTestAllPresetsButton.disabled = true;
      }

      try {
        const response = await requestJson("/api/auth/api-presets");
        if (requestRevision !== apiPresetMutationRevision) {
          return;
        }
        state.apiPresets = response.presets;
        prunePresetTestResults(state.apiPresetTestResults, response.presets, state.apiPresetsTesting);
        state.upstreamProxy = normalizeUpstreamProxySettings(response.upstream_proxy);
        renderUpstreamProxyToggles();
        state.apiPresetsLoaded = true;
        if (
          state.editingApiPresetId &&
          !response.presets.some((preset) => preset.id === state.editingApiPresetId)
        ) {
          resetApiPresetForm("正在编辑的 API 预设已不存在。", "warn");
        }
        apiCurrentFileEl.textContent = response.auth_file;
        apiConfigFileEl.textContent = response.config_file;
        apiPresetFileEl.textContent = response.preset_file;
        apiCurrentTargetEl.textContent = formatCurrentApiHeadline(response);
        renderApiPresets(response.presets);
        refreshBaseUrlPresetOptions();
        refreshConfigOverrideDatalists();
        if (apiTestAllPresetsButton) {
          apiTestAllPresetsButton.disabled = response.presets.length === 0;
        }
        setInlineStatus(apiManagerStatusEl, "", "ok", true);
      } catch (error) {
        state.apiPresets = [];
        state.apiPresetsLoaded = false;
        apiPresetListEl.textContent = "";
        refreshBaseUrlPresetOptions();
        apiCurrentFileEl.textContent = "读取失败";
        apiConfigFileEl.textContent = "读取失败";
        apiCurrentTargetEl.textContent = "读取失败";
        apiPresetFileEl.textContent = "读取失败";
        refreshConfigOverrideDatalists();
        if (apiTestAllPresetsButton) {
          apiTestAllPresetsButton.disabled = true;
        }
        setInlineStatus(apiManagerStatusEl, error.message, "warn", false);
      } finally {
        state.apiPresetsLoading = false;
        if (apiPresetReloadPending) {
          apiPresetReloadPending = false;
          void loadApiPresets();
        }
      }
    }

    function ensureApiPresetsLoaded() {
      if (state.apiPresetsLoaded || state.apiPresetsLoading) {
        return;
      }

      loadApiPresets();
    }

    function setApiPresetEditingState(presetId = "") {
      state.editingApiPresetId = presetId;
      const editingPreset = state.apiPresets.find((preset) => preset.id === presetId);
      const canDuplicate = Boolean(presetId)
        && editingPreset?.access_mode !== "chatgpt_oauth";
      apiSavePresetButton.textContent = presetId ? "编辑" : "新增";
      apiSaveAsNewPresetButton.disabled = !canDuplicate;
      apiSaveAsNewPresetButton.hidden = !canDuplicate;
      apiSaveAsNewPresetButton.textContent = "新增";
      apiApplyEditedPresetButton.disabled = !presetId;
      apiClearInputButton.textContent = presetId ? "取消编辑" : "清空";
    }

    function showApiPresetEditor({ focus = false } = {}) {
      if (apiPresetEditorPanelEl) {
        apiPresetEditorPanelEl.hidden = false;
      }
      if (focus) {
        window.requestAnimationFrame(() => {
          apiPresetNameEl.focus();
          apiPresetNameEl.select();
          apiPresetNameEl.scrollIntoView({ behavior: "smooth", block: "nearest" });
        });
      }
    }

    function hideApiPresetEditor() {
      if (apiPresetEditorPanelEl) {
        apiPresetEditorPanelEl.hidden = true;
      }
    }

    function syncApiManagementUrlField({
      useBaseUrl = apiManagementUrlSameAsBaseInputEl?.checked ?? true,
      syncValue = false,
      focusInput = false,
    } = {}) {
      if (apiManagementUrlSameAsBaseInputEl) {
        apiManagementUrlSameAsBaseInputEl.checked = useBaseUrl;
      }

      if (apiManagementUrlPanelEl) {
        apiManagementUrlPanelEl.hidden = useBaseUrl;
      }

      if (!apiManagementUrlInputEl) {
        return;
      }

      if (useBaseUrl) {
        if (syncValue) {
          apiManagementUrlInputEl.value = apiBaseUrlInputEl?.value.trim() ?? "";
        }
        return;
      }

      if (!apiManagementUrlInputEl.value.trim()) {
        apiManagementUrlInputEl.value = apiBaseUrlInputEl?.value.trim() ?? "";
      }

      if (focusInput) {
        window.requestAnimationFrame(() => {
          apiManagementUrlInputEl.focus();
          apiManagementUrlInputEl.select();
        });
      }
    }

    function resetApiPresetForm(message = "输入内容已清空。", tone = "muted", { hideEditor = true } = {}) {
      apiPresetNameEl.value = "";
      if (apiPresetRowNumberEl) {
        apiPresetRowNumberEl.value = "";
        apiPresetRowNumberEl.disabled = true;
        apiPresetRowNumberEl.max = "";
      }
      apiKeyInputEl.value = "";
      apiKeyInputEl.readOnly = false;
      apiBaseUrlInputEl.value = "";
      apiBaseUrlInputEl.readOnly = false;
      if (apiWireApiInputEl) {
        apiWireApiInputEl.value = "responses";
        apiWireApiInputEl.disabled = false;
      }
      if (apiResponsesProxyInputEl) {
        apiResponsesProxyInputEl.value = "direct";
        apiResponsesProxyInputEl.disabled = false;
      }
      if (apiModelInputEl) {
        apiModelInputEl.value = "";
        apiModelInputEl.readOnly = false;
      }
      apiManagementUrlInputEl.value = "";
      if (apiApplyUpstreamProxyOnSwitchInputEl) {
        apiApplyUpstreamProxyOnSwitchInputEl.checked = false;
        apiApplyUpstreamProxyOnSwitchInputEl.disabled = false;
      }
      state.apiApplyProxyManuallyChanged = false;
      if (apiTerminalEnvInputEl) {
        apiTerminalEnvInputEl.value = "";
      }
      if (apiTerminalStartupScriptInputEl) {
        apiTerminalStartupScriptInputEl.value = "";
      }
      if (apiTerminalStartupDetailsEl) {
        apiTerminalStartupDetailsEl.open = false;
      }
      syncApiManagementUrlField({
        useBaseUrl: true,
        syncValue: false,
      });
      renderConfigOverrideEditor(apiConfigOverrideControls, [], { open: false });
      syncApiApplyProxyRecommendation();
      setApiPresetEditingState("");
      if (hideEditor) {
        hideApiPresetEditor();
      }
      updateStatus(apiFormStatusEl, message || API_FORM_DEFAULT_STATUS, tone);
    }

    function startNewApiPreset() {
      resetApiPresetForm("正在新增 API 预设。", "info", { hideEditor: false });
      if (apiPresetRowNumberEl) {
        const nextRowNumber = apiPresetVisibleOrder().length + 1;
        apiPresetRowNumberEl.value = String(nextRowNumber);
        apiPresetRowNumberEl.max = String(nextRowNumber);
        apiPresetRowNumberEl.disabled = false;
      }
      showApiPresetEditor({ focus: true });
    }

    function editApiPreset(presetId) {
      const preset = state.apiPresets.find((item) => item.id === presetId);
      if (!preset) {
        updateStatus(apiManagerStatusEl, "找不到要编辑的 API 预设。", "warn");
        return;
      }

      apiPresetNameEl.value = preset.name || preset.provider_name || "";
      if (apiPresetRowNumberEl) {
        const rowNumber = apiPresetVisibleRowNumber(preset.id);
        apiPresetRowNumberEl.value = rowNumber > 0 ? String(rowNumber) : "";
        apiPresetRowNumberEl.max = String(Math.max(apiPresetVisibleOrder().length, 1));
        apiPresetRowNumberEl.disabled = apiPresetVisibleOrder().length <= 1;
      }
      apiKeyInputEl.value = preset.api_key ?? "";
      apiKeyInputEl.readOnly = preset.access_mode === "chatgpt_oauth";
      apiBaseUrlInputEl.value = preset.base_url;
      apiBaseUrlInputEl.readOnly = preset.access_mode === "chatgpt_oauth";
      if (apiWireApiInputEl) {
        apiWireApiInputEl.value = preset.wire_api || "responses";
        apiWireApiInputEl.disabled = preset.access_mode === "chatgpt_oauth";
      }
      if (apiResponsesProxyInputEl) {
        apiResponsesProxyInputEl.value = preset.responses_proxy || "direct";
        apiResponsesProxyInputEl.disabled = preset.access_mode === "chatgpt_oauth";
      }
      apiManagementUrlInputEl.value = preset.management_url ?? "";
      if (apiApplyUpstreamProxyOnSwitchInputEl) {
        apiApplyUpstreamProxyOnSwitchInputEl.checked = Boolean(preset.apply_upstream_proxy_on_switch);
        apiApplyUpstreamProxyOnSwitchInputEl.disabled = preset.access_mode === "chatgpt_oauth";
      }
      if (apiTerminalEnvInputEl) {
        apiTerminalEnvInputEl.value = formatTerminalStartupEnvVars(preset.terminal_env);
      }
      if (apiTerminalStartupScriptInputEl) {
        apiTerminalStartupScriptInputEl.value = preset.terminal_startup_script || "";
      }
      if (apiTerminalStartupDetailsEl) {
        apiTerminalStartupDetailsEl.open = apiPresetHasTerminalStartupSettings(preset);
      }
      syncApiManagementUrlField({
        useBaseUrl: !preset.management_url || preset.management_url === preset.base_url,
        syncValue: !preset.management_url || preset.management_url === preset.base_url,
      });
      const configOverrides = normalizePresetConfigOverrides(preset);
      const presetModel = extractModelFromOverrides(configOverrides);
      if (apiModelInputEl) {
        apiModelInputEl.value = presetModel;
        apiModelInputEl.readOnly = preset.access_mode === "chatgpt_oauth";
      }
      const editorOverrides = overridesWithoutModel(configOverrides);
      renderConfigOverrideEditor(apiConfigOverrideControls, editorOverrides, {
        open: editorOverrides.length > 0,
      });
      state.apiApplyProxyManuallyChanged = true;
      syncApiApplyProxyRecommendation();
      setApiPresetEditingState(preset.id);
      showApiPresetEditor({ focus: true });
      updateStatus(
        apiFormStatusEl,
        preset.access_mode === "chatgpt_oauth"
          ? `正在编辑 OAuth 代理预设：${preset.name}。凭据和路由由导入账号管理。`
          : `正在编辑 API 预设：${preset.name}`,
        "info",
      );
    }

    async function saveApiPreset() {
      return saveApiPresetWithMode(false);
    }

    async function saveApiPresetAsNew() {
      return saveApiPresetWithMode(true);
    }

    async function saveApiPresetWithMode(forceNewPreset = false) {
      const editingPresetId = state.editingApiPresetId;
      const editingPreset = state.apiPresets.find((item) => item.id === editingPresetId);
      const isChatgptOauth = editingPreset?.access_mode === "chatgpt_oauth";
      const name = apiPresetNameEl.value.trim();
      const apiKey = apiKeyInputEl.value.trim();
      const baseUrl = apiBaseUrlInputEl.value.trim();
      const useBaseUrlAsManagement = apiManagementUrlSameAsBaseInputEl?.checked ?? true;
      const wireApi = apiWireApiInputEl?.value || "responses";
      const responsesProxy = apiResponsesProxyInputEl?.value || "direct";
      const managementUrl = useBaseUrlAsManagement
        ? baseUrl
        : apiManagementUrlInputEl.value.trim();
      const rawConfigOverrides = collectConfigOverrideEditorValues(apiConfigOverrideControls);
      const model = (apiModelInputEl?.value || "").trim();
      const configOverrides = isChatgptOauth
        ? overridesWithoutModel(rawConfigOverrides)
        : mergeModelIntoOverrides(rawConfigOverrides);

      if (!apiKey) {
        updateStatus(apiFormStatusEl, "先填写 API Key。", "warn");
        return;
      }
      if (!baseUrl) {
        updateStatus(apiFormStatusEl, "先填写 Base URL。", "warn");
        return;
      }
      // OAuth presets are exempt: their model is fixed by the ChatGPT backend
      // and must not be overridden.
      if (!isChatgptOauth && !model) {
        updateStatus(apiFormStatusEl, "先填写模型名称，确保切换该预设时 model 与 provider 一起变更。", "warn");
        return;
      }

      const isEditing = Boolean(editingPresetId) && !forceNewPreset;
      const maxTargetRowNumber = apiPresetVisibleOrder().length + (isEditing ? 0 : 1);
      const targetRowNumber = apiPresetRowNumberEl
        ? Math.max(1, Math.min(
          maxTargetRowNumber,
          Math.trunc(Number(apiPresetRowNumberEl.value) || 1),
        ))
        : null;
      if (isEditing && !confirmEditedPresetOverwrite({
        presetKind: "API 预设",
        presetName: editingPreset?.name || name,
      })) {
        return;
      }
      if (isEditing && !confirmApiApplyProxyRecommendationBeforeSave()) {
        return;
      }

      updateStatus(
        apiFormStatusEl,
        isEditing ? "正在更新 API 预设…" : "正在新增 API 预设…",
        "info",
      );
      apiSavePresetButton.disabled = true;
      apiSaveAsNewPresetButton.disabled = true;
      apiApplyEditedPresetButton.disabled = true;
      apiClearInputButton.disabled = true;

      try {
        if (isEditing && targetRowNumber) {
          try {
            await moveApiPresetToRowNumber(editingPresetId, targetRowNumber);
          } catch (error) {
            updateStatus(apiFormStatusEl, `保存行号失败：${error.message}`, "warn");
            return;
          }
        }
        const response = await requestJson(
          isEditing
            ? `/api/auth/api-presets/${encodeURIComponent(editingPresetId)}`
            : "/api/auth/api-presets",
          {
            method: isEditing ? "PUT" : "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify({
              name,
              provider_name: name || null,
              api_key: apiKey,
              access_mode: editingPreset?.access_mode || null,
              base_url: baseUrl,
              wire_api: wireApi,
              responses_proxy: responsesProxy,
              management_url: managementUrl || null,
              management_url_same_as_base: useBaseUrlAsManagement,
              apply_upstream_proxy_on_switch: readApiApplyUpstreamProxyOnSwitch(),
              terminal_env: parseTerminalStartupEnvInput(apiTerminalEnvInputEl?.value || ""),
              terminal_startup_script: apiTerminalStartupScriptInputEl.value,
              config_overrides: configOverrides,
            }),
          },
        );
        if (response?.preset?.id) {
          apiPresetMutationRevision += 1;
          const responseIndex = state.apiPresets.findIndex(
            (preset) => preset.id === response.preset.id,
          );
          if (responseIndex >= 0) {
            const nextPresets = state.apiPresets.slice();
            nextPresets[responseIndex] = response.preset;
            state.apiPresets = nextPresets;
          } else {
            state.apiPresets = [...state.apiPresets, response.preset];
          }
          renderApiPresets(state.apiPresets);
          refreshBaseUrlPresetOptions();
          refreshConfigOverrideDatalists();
        }
        if (!isEditing && targetRowNumber && response?.preset?.id) {
          const currentIds = new Set(state.apiPresets.map((preset) => preset.id));
          const nextPresets = currentIds.has(response.preset.id)
            ? state.apiPresets
            : [...state.apiPresets, response.preset];
          state.apiPresets = nextPresets;
          try {
            await moveApiPresetToRowNumber(response.preset.id, targetRowNumber);
          } catch (error) {
            await loadApiPresets();
            updateStatus(
              apiFormStatusEl,
              `已新增预设：${response.preset.name}；但保存行号失败：${error.message}`,
              "warn",
            );
            return;
          }
        }
        resetApiPresetForm(
          `${isEditing ? "已更新" : "已新增"}预设：${response.preset.name}`,
          "ok",
        );
        await loadApiPresets();
      } catch (error) {
        updateStatus(apiFormStatusEl, error.message, "warn");
      } finally {
        apiSavePresetButton.disabled = false;
        apiSaveAsNewPresetButton.disabled = false;
        apiApplyEditedPresetButton.disabled = false;
        apiClearInputButton.disabled = false;
        setApiPresetEditingState(state.editingApiPresetId);
      }
    }

    async function applyApiPreset(
      presetId,
      statusElement = apiManagerStatusEl,
      { respectSavedProxyPreference = true } = {},
    ) {
      updateStatus(statusElement, "正在切换 API 预设并校正 config.toml…", "info");

      try {
        const params = new URLSearchParams();
        if (!respectSavedProxyPreference) {
          params.set("respect_saved_proxy_preference", "false");
        }
        // Pass the current project directory so a project-local .codex/config.toml
        // that overrides the global config gets synced too.
        const projectPath = state.currentDirectory?.display_path || state.workspaceDir || "";
        if (projectPath) {
          params.set("project_path", projectPath);
        }
        const query = params.toString() ? `?${params.toString()}` : "";
        const response = await requestJson(
          `/api/auth/api-presets/${encodeURIComponent(presetId)}/apply${query}`,
          {
            method: "PUT",
          },
        );
        if (response.deferred) {
          updateStatus(
            statusElement,
            `已登记 API 预设切换：${response.name}。当前指定 Agent 退出并恢复原配置后，将实际写入该预设。`,
            "info",
          );
          await refreshAuthPanels();
          return false;
        }
        const note = response.local_config_file
          ? `（已同步项目本地配置：${response.local_config_file}）`
          : "";
        updateStatus(statusElement, `已应用 API 预设：${response.name}${note}`, "ok");
        await refreshAuthPanels();
        return true;
      } catch (error) {
        updateStatus(statusElement, error.message, "warn");
        return false;
      }
    }

    async function applyApiPresetAndLaunch(presetId) {
      const targetPreset = state.apiPresets.find((item) => item.id === presetId);
      if (!targetPreset) {
        updateStatus(apiManagerStatusEl, "找不到要切换的 API 预设。", "warn");
        return;
      }

      updateStatus(apiManagerStatusEl, `正在切换到 ${targetPreset.name} 并启动 Codex…`, "info");
      const applied = await applyApiPreset(presetId, apiManagerStatusEl);
      if (!applied) {
        return;
      }

      const currentDir = state.currentDirectory?.display_path
        ? state.currentDirectory.display_path
        : state.workspaceDir || "";

      let sessionId = "";
      let sessionPath = currentDir;
      try {
        const session = await requestJson("/api/terminal/sessions", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ path: currentDir }),
        });
        sessionId = session.id || "";
        sessionPath = session.path || currentDir;
      } catch (error) {
        updateStatus(apiManagerStatusEl, `创建终端失败：${error.message}`, "warn");
        return;
      }

      const params = new URLSearchParams();
      if (sessionPath) {
        params.set("path", sessionPath);
      }
      if (sessionId) {
        params.set("session", sessionId);
      } else {
        params.set("fresh", "1");
      }
      params.set("run", "codex");
      window.location.assign(`/terminal?${params.toString()}`);
    }

    async function applyEditingApiPreset() {
      if (!state.editingApiPresetId) {
        updateStatus(apiFormStatusEl, "先点击一条 API 预设的编辑。", "warn");
        return;
      }

      await applyApiPreset(state.editingApiPresetId, apiFormStatusEl);
    }

    async function testApiPreset(presetId, presetName) {
      state.apiPresetsTesting.add(presetId);
      presetTestPopup.hide();
      renderApiPresets(state.apiPresets);
      try {
        const response = await requestJson(`/api/auth/api-presets/${encodeURIComponent(presetId)}/test`, {
          method: "POST",
        });
        const result = normalizeTestResult(response.result, {
          presetId,
          fallbackName: presetName,
        });
        state.apiPresetTestResults.set(presetId, result);
      } catch (error) {
        state.apiPresetTestResults.set(
          presetId,
          errorTestResult(error, { presetId, fallbackName: presetName }),
        );
      } finally {
        state.apiPresetsTesting.delete(presetId);
        presetTestPopup.hide();
        renderApiPresets(state.apiPresets);
      }
    }

    async function testAllApiPresets() {
      if (!state.apiPresets.length) {
        updateStatus(apiManagerStatusEl, "没有可测试的 API 预设。", "warn");
        return;
      }

      state.apiPresets.forEach((preset) => state.apiPresetsTesting.add(preset.id));
      setButtonBusy(apiTestAllPresetsButton, true, "测试中…");
      presetTestPopup.hide();
      renderApiPresets(state.apiPresets);
      try {
        const response = await requestJson("/api/auth/api-presets/test-all", {
          method: "POST",
        });
        const results = Array.isArray(response.results) ? response.results : [];
        results.forEach((raw) => {
          const normalized = normalizeTestResult(raw, { presetId: raw.preset_id });
          state.apiPresetTestResults.set(normalized.preset_id, normalized);
        });
      } catch (error) {
        state.apiPresets.forEach((preset) => {
          state.apiPresetTestResults.set(
            preset.id,
            errorTestResult(error, { presetId: preset.id, fallbackName: preset.name }),
          );
        });
      } finally {
        state.apiPresetsTesting.clear();
        setButtonBusy(apiTestAllPresetsButton, false);
        presetTestPopup.hide();
        renderApiPresets(state.apiPresets);
        if (apiTestAllPresetsButton) {
          apiTestAllPresetsButton.disabled = state.apiPresets.length === 0;
        }
      }
    }

    async function deleteApiPreset(presetId, presetName) {
      if (!window.confirm(`确定删除 API 预设"${presetName}"吗？`)) {
        return;
      }

      updateStatus(apiManagerStatusEl, "正在删除 API 预设…", "info");

      try {
        await requestJson(`/api/auth/api-presets/${encodeURIComponent(presetId)}`, {
          method: "DELETE",
        });
        if (state.editingApiPresetId === presetId) {
          resetApiPresetForm("正在编辑的 API 预设已删除。", "warn");
        }
        updateStatus(apiManagerStatusEl, `已删除预设：${presetName}`, "ok");
        await loadApiPresets();
      } catch (error) {
        updateStatus(apiManagerStatusEl, error.message, "warn");
      }
    }

    async function importApiAccountsFromText(rawText) {
      const trimmedText = (rawText || "").trim();
      if (!trimmedText) {
        if (apiAccountImportStatusEl) {
          updateStatus(apiAccountImportStatusEl, "先粘贴账号 JSON。", "warn");
        }
        return false;
      }

      if (apiAccountImportSubmitButton) apiAccountImportSubmitButton.disabled = true;
      if (apiAccountImportTextButton) apiAccountImportTextButton.disabled = true;

      try {
        const result = await importAuthAccounts(trimmedText);
        if (typeof refreshAuthPanels === "function") {
          await refreshAuthPanels();
        }
        const statusEl = apiAccountImportStatusEl || apiFormStatusEl;
        updateStatus(
          statusEl,
          result.saved_count > 1
            ? `已导入 ${result.saved_count} 个 API 预设。`
            : `已导入预设：${result.saved_names[0] || result.saved_count}。`,
          "ok",
        );
        if (apiAccountImportDialogEl && apiAccountImportDialogEl.open) {
          apiAccountImportDialogEl.close();
        }
        return true;
      } catch (error) {
        const statusEl = apiAccountImportStatusEl || apiFormStatusEl;
        updateStatus(statusEl, error.message, "warn");
        return false;
      } finally {
        if (apiAccountImportSubmitButton) apiAccountImportSubmitButton.disabled = false;
        if (apiAccountImportTextButton) apiAccountImportTextButton.disabled = false;
      }
    }

    async function importApiAccountsFromFiles(sourceFiles) {
      const files = Array.from(sourceFiles || []).filter(Boolean);
      if (!files.length) {
        return false;
      }
      for (const sourceFile of files) {
        const fileName = typeof sourceFile?.name === "string" ? sourceFile.name.toLowerCase() : "";
        if (!/\.(json|zip|tar|tar\.gz|tgz|gz)$/.test(fileName)) {
          const statusEl = apiManagerStatusEl || apiAccountImportStatusEl || apiFormStatusEl;
          updateStatus(statusEl, "请选择 JSON、ZIP、TAR、TAR.GZ、TGZ 或 GZ 文件。", "warn");
          return false;
        }
      }
      const totalSize = files.reduce(
        (sum, sourceFile) => sum + (Number.isFinite(sourceFile?.size) ? sourceFile.size : 0),
        0,
      );
      if (totalSize > 32 * 1024 * 1024) {
        const statusEl = apiManagerStatusEl || apiAccountImportStatusEl || apiFormStatusEl;
        updateStatus(statusEl, "导入文件总大小不能超过 32 MiB。", "warn");
        return false;
      }

      if (apiAccountImportFileButton) apiAccountImportFileButton.disabled = true;
      const statusEl = apiManagerStatusEl || apiAccountImportStatusEl || apiFormStatusEl;
      updateStatus(
        statusEl,
        files.length > 1 ? `正在导入 ${files.length} 个账号文件…` : `正在导入 ${files[0].name || "账号文件"}…`,
        "info",
      );

      try {
        const result = await importAuthAccountFiles(files);
        if (typeof refreshAuthPanels === "function") {
          await refreshAuthPanels();
        }
        const errorCount = Array.isArray(result.errors) ? result.errors.length : 0;
        const message = result.saved_count > 1
          ? `已导入 ${result.saved_count} 个 API 预设。`
          : `已导入预设：${result.saved_names?.[0] || result.saved_count}。`;
        updateStatus(
          statusEl,
          errorCount ? `${message} ${errorCount} 个文件未导入。` : message,
          errorCount ? "warn" : "ok",
        );
        return true;
      } catch (error) {
        updateStatus(statusEl, error.message || "导入文件失败。", "warn");
        return false;
      } finally {
        if (apiAccountImportFileButton) apiAccountImportFileButton.disabled = false;
      }
    }

    async function importApiAccountsFromFile(sourceFile) {
      return importApiAccountsFromFiles(sourceFile ? [sourceFile] : []);
    }

    initializeApiPresetGroupMode();
    initializeApiPresetListControls();

    return {
      applyApiPreset,
      applyApiPresetAndLaunch,
      applyEditingApiPreset,
      deleteApiPreset,
      editApiPreset,
      ensureApiPresetsLoaded,
      importApiAccountsFromText,
      importApiAccountsFromFile,
      importApiAccountsFromFiles,
      loadApiPresets,
      renderApiPresets,
      resetApiPresetForm,
      saveApiPreset,
      saveApiPresetAsNew,
      startNewApiPreset,
      syncApiApplyProxyRecommendation,
      syncApiManagementUrlField,
      testAllApiPresets,
      testApiPreset,
      warnApiApplyProxyRecommendationIfNeeded,
    };
  }

  globalThis.WebClxApiManager = Object.freeze({ create: createApiManager });
})();
