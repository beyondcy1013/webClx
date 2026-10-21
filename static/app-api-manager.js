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
      renderPresetCompactList,
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
      createEditablePresetNameCell,
      createTextCell,
      createExternalUrlCell,
      formatDateTime,
      formatTerminalStartupEnvVars,
      parseTerminalStartupEnvInput,
      normalizeTerminalStartupEnvVars,
      apiBaseUrlMatchesSelectedAutoProxyProvider,
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
      apiCurrentFileEl,
      apiConfigFileEl,
      apiCurrentTargetEl,
      apiPresetFileEl,
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
      apiPresetEditorModeLabelEl,
      apiSavePresetButton,
      apiSaveAsNewPresetButton,
      apiApplyEditedPresetButton,
      apiClearInputButton,
      apiFormStatusEl,
    } = elements;
    let apiPresetFormSnapshot = null;

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
    }

    function apiPresetIsFavorite(preset) {
      return preset?.favorite === true;
    }

    function sortApiPresetsWithFavoritesFirst(presets) {
      const source = Array.isArray(presets) ? presets : [];
      const favorites = source.filter((preset) => apiPresetIsFavorite(preset));
      if (favorites.length === 0) {
        return source.slice();
      }
      const favoriteIds = new Set(favorites.map((preset) => preset?.id));
      return [
        ...favorites,
        ...source.filter((preset) => !favoriteIds.has(preset?.id)),
      ];
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
        { key: "favorite", type: "boolean", defaultDirection: "desc", getValue: (preset) => apiPresetIsFavorite(preset) },
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
      const orderedPresets = sortApiPresetsWithFavoritesFirst(presets);
      const groups = [];
      const groupsByKey = new Map();
      orderedPresets.forEach((preset, index) => {
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
        setInlineStatus(apiFormStatusEl, "API 预设顺序已保存。", "success");
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
        setInlineStatus(apiFormStatusEl, "API 预设顺序已保存。", "success");
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
          label: preset.favorite ? "取消收藏" : "收藏",
          handler: () => toggleApiPresetFavorite(preset.id),
        },
        {
          label: "切换并启动",
          handler: () => applyApiPresetAndLaunch(preset.id),
        },
        {
          label: "测试",
          disabled: state.apiPresetsTesting.has(preset.id),
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
      appendApiPresetMobileDetail(list, "收藏", preset.favorite ? "已收藏" : "未收藏");
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

    function renderApiPresets(presets) {
      // API accounts are owned by the LLM_API unified list; the standalone API
      // detail table is intentionally gone. Preserve favorite/order selection
      // cleanup and the model datalist used by the on-demand editor.
      const validIds = new Set(presets.map((preset) => preset.id));
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
      setInlineStatus(apiFormStatusEl, "", "info", true);
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
        setInlineStatus(apiFormStatusEl, "", "ok", true);
      } catch (error) {
        state.apiPresets = [];
        state.apiPresetsLoaded = false;
        refreshBaseUrlPresetOptions();
        apiCurrentFileEl.textContent = "读取失败";
        apiConfigFileEl.textContent = "读取失败";
        apiCurrentTargetEl.textContent = "读取失败";
        apiPresetFileEl.textContent = "读取失败";
        refreshConfigOverrideDatalists();
        setInlineStatus(apiFormStatusEl, error.message, "warn", false);
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
      if (apiPresetEditorModeLabelEl) {
        apiPresetEditorModeLabelEl.textContent = presetId ? "编辑预设" : "新增预设";
      }
      apiSavePresetButton.textContent = "保存";
      apiSaveAsNewPresetButton.disabled = !canDuplicate;
      apiSaveAsNewPresetButton.hidden = !canDuplicate;
      apiSaveAsNewPresetButton.textContent = "另存为副本";
      apiApplyEditedPresetButton.disabled = !presetId;
      apiClearInputButton.textContent = presetId ? "取消编辑" : "清空";
    }

    function captureApiPresetFormSnapshot() {
      return JSON.stringify({
        name: apiPresetNameEl.value,
        apiKey: apiKeyInputEl.value,
        baseUrl: apiBaseUrlInputEl.value,
        wireApi: apiWireApiInputEl?.value,
        responsesProxy: apiResponsesProxyInputEl?.value,
        managementUrl: apiManagementUrlInputEl.value,
        managementUrlSameAsBase: Boolean(apiManagementUrlSameAsBaseInputEl?.checked),
        model: apiModelInputEl?.value,
        applyProxy: Boolean(apiApplyUpstreamProxyOnSwitchInputEl?.checked),
        terminalEnv: apiTerminalEnvInputEl?.value,
        terminalStartupScript: apiTerminalStartupScriptInputEl.value,
        configOverrides: collectConfigOverrideEditorValues(apiConfigOverrideControls),
      });
    }

    function hasApiPresetUnsavedChanges() {
      return Boolean(apiPresetFormSnapshot && captureApiPresetFormSnapshot() !== apiPresetFormSnapshot);
    }

    function confirmDiscardApiPresetChanges() {
      if (!hasApiPresetUnsavedChanges()) return true;
      return window.confirm("当前 API 预设编辑内容尚未保存。\n\n确定要放弃这些修改吗？");
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
      apiPresetFormSnapshot = null;
    }

    function startNewApiPreset() {
      if (!confirmDiscardApiPresetChanges()) return;
      resetApiPresetForm("正在新增 API 预设。", "info", { hideEditor: false });
      if (apiPresetRowNumberEl) {
        const nextRowNumber = apiPresetVisibleOrder().length + 1;
        apiPresetRowNumberEl.value = String(nextRowNumber);
        apiPresetRowNumberEl.max = String(nextRowNumber);
        apiPresetRowNumberEl.disabled = false;
      }
      apiPresetFormSnapshot = captureApiPresetFormSnapshot();
      showApiPresetEditor({ focus: true });
    }

    function editApiPreset(presetId) {
      if (state.editingApiPresetId === presetId && apiPresetEditorPanelEl && !apiPresetEditorPanelEl.hidden) {
        showApiPresetEditor({ focus: true });
        return;
      }
      if (!confirmDiscardApiPresetChanges()) return;
      const preset = state.apiPresets.find((item) => item.id === presetId);
      if (!preset) {
        updateStatus(apiFormStatusEl, "找不到要编辑的 API 预设。", "warn");
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
      apiPresetFormSnapshot = captureApiPresetFormSnapshot();
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
          if (isEditing && targetRowNumber) {
            try {
              await moveApiPresetToRowNumber(response.preset.id, targetRowNumber);
            } catch (error) {
              await loadApiPresets();
              updateStatus(
                apiFormStatusEl,
                `已更新预设：${response.preset.name}；但保存行号失败：${error.message}`,
                "warn",
              );
              return;
            }
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
        await window.loadCodexAccounts?.();
        apiPresetFormSnapshot = null;
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
      statusElement = apiFormStatusEl,
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
            `API 预设未被立即写入：${response.name}。已停止后续启动，请重试。`,
            "info",
          );
          await refreshAuthPanels();
          return false;
        }
        const note = response.local_config_file
          ? `（已同步项目本地配置：${response.local_config_file}）`
          : "";
        const daemonNote = response.codex_daemon_restarted
          ? "（已重启 Codex 后台守护进程使切换生效）"
          : "";
        updateStatus(statusElement, `已应用 API 预设：${response.name}${note}${daemonNote}`, "ok");
        await refreshAuthPanels();
        return true;
      } catch (error) {
        updateStatus(statusElement, error.message, "warn");
        return false;
      }
    }

    function cancelApiPresetEdit() {
      if (!confirmDiscardApiPresetChanges()) return false;
      resetApiPresetForm(state.editingApiPresetId ? "已取消编辑。" : "输入内容已清空。", "muted");
      return true;
    }

    async function toggleApiPresetFavorite(presetId) {
      const preset = state.apiPresets.find((item) => item.id === presetId);
      if (!preset) {
        setInlineStatus(apiFormStatusEl, "找不到要收藏的 API 预设。", "warn");
        return;
      }
      try {
        const response = await requestJson(
          `/api/auth/api-presets/${encodeURIComponent(presetId)}/favorite`,
          { method: "PUT" },
        );
        if (response?.preset?.id) {
          const nextPresets = state.apiPresets.map((item) => (
            item.id === response.preset.id ? response.preset : item
          ));
          state.apiPresets = nextPresets;
          state.presetTableSort?.delete?.("api");
          renderApiPresets(state.apiPresets);
          refreshBaseUrlPresetOptions();
          setInlineStatus(
                  `${response.preset.favorite ? "已收藏" : "已取消收藏"}：${response.preset.name}`,
            "success",
          );
        }
      } catch (error) {
        setInlineStatus(apiFormStatusEl, error.message, "warn");
      }
    }

    async function renameApiPreset(presetId, nextName) {
      const preset = state.apiPresets.find((item) => item.id === presetId);
      if (!preset) {
        updateStatus(apiFormStatusEl, "找不到要改名的 API 预设。", "warn");
        return;
      }
      if (state.editingApiPresetId === presetId && hasApiPresetUnsavedChanges()) {
        updateStatus(apiFormStatusEl, "请先保存或取消当前预设的未保存编辑。", "warn");
        return;
      }

      updateStatus(apiFormStatusEl, `正在重命名 API 预设：${preset.name}…`, "info");
      try {
        const response = await requestJson(
          `/api/auth/api-presets/${encodeURIComponent(presetId)}`,
          {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              name: nextName,
              provider_name: preset.provider_name || nextName,
              api_key: preset.api_key,
              access_mode: preset.access_mode || null,
              base_url: preset.base_url,
              wire_api: preset.wire_api || "responses",
              responses_proxy: preset.responses_proxy || "direct",
              management_url: preset.management_url || null,
              management_url_same_as_base: !preset.management_url || preset.management_url === preset.base_url,
              apply_upstream_proxy_on_switch: preset.apply_upstream_proxy_on_switch,
              terminal_env: preset.terminal_env || [],
              terminal_startup_script: preset.terminal_startup_script || "",
              config_overrides: preset.config_overrides || [],
            }),
          },
        );
        if (response?.preset?.id) {
          const nextPresets = state.apiPresets.map((item) => (
            item.id === response.preset.id ? response.preset : item
          ));
          state.apiPresets = nextPresets;
          renderApiPresets(state.apiPresets);
          refreshBaseUrlPresetOptions();
          refreshConfigOverrideDatalists();
        }
        updateStatus(apiFormStatusEl, `已重命名预设：${response.preset.name}`, "ok");
      } catch (error) {
        renderApiPresets(state.apiPresets);
        updateStatus(apiFormStatusEl, error.message, "warn");
      }
    }

    async function applyApiPresetAndLaunch(presetId) {
      const targetPreset = state.apiPresets.find((item) => item.id === presetId);
      if (!targetPreset) {
        updateStatus(apiFormStatusEl, "找不到要切换的 API 预设。", "warn");
        return;
      }

      updateStatus(apiFormStatusEl, `正在切换到 ${targetPreset.name} 并启动 Codex…`, "info");
      const applied = await applyApiPreset(presetId, apiFormStatusEl);
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
        updateStatus(apiFormStatusEl, `创建终端失败：${error.message}`, "warn");
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

    async function testApiPreset(presetId, presetName, statusElement = apiFormStatusEl) {
      if (state.apiPresetsTesting.has(presetId)) return;
      state.apiPresetsTesting.add(presetId);
      presetTestPopup.hide();
      updateStatus(statusElement, `正在测试 API 预设“${presetName || presetId}”…`, "info");
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
        updateStatus(
          statusElement,
          `API 预设“${result.name || presetName || presetId}”测试${result.ok ? "通过" : "失败"}。`,
          result.ok ? "ok" : "warn",
        );
      } catch (error) {
        const result = errorTestResult(error, { presetId, fallbackName: presetName });
        state.apiPresetTestResults.set(presetId, result);
        updateStatus(
          statusElement,
          `API 预设“${presetName || presetId}”测试请求失败：${error.message}`,
          "warn",
        );
      } finally {
        state.apiPresetsTesting.delete(presetId);
        presetTestPopup.hide();
        renderApiPresets(state.apiPresets);
      }
    }

    async function testAllApiPresets() {
      if (!state.apiPresets.length) {
        updateStatus(apiFormStatusEl, "没有可测试的 API 预设。", "warn");
        return;
      }

      state.apiPresets.forEach((preset) => state.apiPresetsTesting.add(preset.id));
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
        presetTestPopup.hide();
        renderApiPresets(state.apiPresets);
      }
    }

    async function deleteApiPreset(presetId, presetName) {
      if (!window.confirm(`确定删除 API 预设"${presetName}"吗？`)) {
        return;
      }

      updateStatus(apiFormStatusEl, "正在删除 API 预设…", "info");

      try {
        await requestJson(`/api/auth/api-presets/${encodeURIComponent(presetId)}`, {
          method: "DELETE",
        });
        if (state.editingApiPresetId === presetId) {
          resetApiPresetForm("正在编辑的 API 预设已删除。", "warn");
        }
        updateStatus(apiFormStatusEl, `已删除预设：${presetName}`, "ok");
        await loadApiPresets();
      } catch (error) {
        updateStatus(apiFormStatusEl, error.message, "warn");
      }
    }

    initializeApiPresetGroupMode();

    return {
      applyApiPreset,
      applyApiPresetAndLaunch,
      applyEditingApiPreset,
      cancelApiPresetEdit,
      deleteApiPreset,
      editApiPreset,
      ensureApiPresetsLoaded,
      loadApiPresets,
      renderApiPresets,
      renameApiPreset,
      toggleApiPresetFavorite,
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
