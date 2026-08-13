(function () {
  function createClaudeManager(deps) {
    const {
      state,
      requestJson,
      updateStatus,
      setInlineStatus,
      setButtonBusy,
      refreshAuthPanels,
      normalizeUpstreamProxySettings,
      renderUpstreamProxyToggles,
      updateUpstreamProxySettings,
      refreshBaseUrlPresetOptions,
      refreshConfigOverrideDatalists,
      renderConfigOverrideEditor,
      collectConfigOverrideEditorValues,
      normalizePresetConfigOverrides,
      collectPresetConfigKeys,
      normalizePresetBaseUrlGroupKey,
      renderClaudePresetTableHeader,
      renderPresetTable,
      movePresetById,
      movePresetOrderWithPersist,
      persistPresetOrder,
      buildPresetConfigValueMap,
      buildPresetConfigCells,
      createActionCell,
      createActionButton,
      createPresetDeleteButton,
      createCurrentIndicatorCell,
      createPresetNameCell,
      createTextCell,
      createExternalUrlCell,
      formatDateTime,
      confirmEditedPresetOverwrite,
      normalizeTestResult,
      errorTestResult,
      prunePresetTestResults,
      presetTestPopup,
      CLAUDE_FORM_DEFAULT_STATUS,
      elements,
    } = deps;
    const {
      claudeManagerStatusEl,
      claudeCurrentFileEl,
      claudeCurrentTargetEl,
      claudePresetFileEl,
      claudeProviderNameInputEl,
      claudeAuthTokenInputEl,
      claudeBaseUrlInputEl,
      claudeManagementUrlInputEl,
      claudeDefaultHaikuModelInputEl,
      claudeDefaultSonnetModelInputEl,
      claudeDefaultOpusModelInputEl,
      claudeThirdPartyModelInputEl,
      claudeModelModeOfficialInputEl,
      claudeModelModeThirdPartyInputEl,
      claudeOfficialModelGroupEl,
      claudeThirdPartyModelGroupEl,
      claudeConfigOverrideControls,
      claudeSavePresetButton,
      claudeSaveAsNewPresetButton,
      claudeApplyEditedPresetButton,
      claudeClearInputButton,
      claudeTestAllPresetsButton,
      claudeFormStatusEl,
      claudePresetListEl,
      claudeSharedModelOptionsEl,
      claudeAccessModeInputEl,
    } = elements;

    function normalizeClaudeModelOptions(options) {
      const values = Array.isArray(options) ? options : [];
      const normalized = [];
      const seen = new Set();

      values.forEach((value) => {
        if (typeof value !== "string") {
          return;
        }
        const trimmed = value.trim();
        if (!trimmed || seen.has(trimmed)) {
          return;
        }
        seen.add(trimmed);
        normalized.push(trimmed);
      });

      return normalized;
    }

    function collectClaudePresetModelOptions(presets = state.claudePresets) {
      const values = [];
      const sources = Array.isArray(presets) ? presets : [];
      sources.forEach((preset) => {
        [
          preset?.default_haiku_model,
          preset?.default_sonnet_model,
          preset?.default_opus_model,
          preset?.third_party_model,
        ].forEach((value) => {
          if (typeof value === "string" && value.trim()) {
            values.push(value.trim());
          }
        });
      });
      return values;
    }

    function parseClaudeModelOptionsInput(rawText) {
      return normalizeClaudeModelOptions(rawText.split(/\r?\n/u));
    }

    function renderClaudeModelOptions(options = state.claudeModelOptions) {
      if (!claudeSharedModelOptionsEl) {
        return;
      }

      const normalized = normalizeClaudeModelOptions(options);
      state.claudeModelOptions = normalized;
      const renderedOptions = normalizeClaudeModelOptions([
        ...normalized,
        ...collectClaudePresetModelOptions(),
      ]);
      claudeSharedModelOptionsEl.textContent = "";

      renderedOptions.forEach((value) => {
        const option = document.createElement("option");
        option.value = value;
        claudeSharedModelOptionsEl.appendChild(option);
      });
    }

    function formatClaudeModelSummary(source) {
      const parts = [
        source.default_haiku_model ? `Haiku ${source.default_haiku_model}` : null,
        source.default_sonnet_model ? `Sonnet ${source.default_sonnet_model}` : null,
        source.default_opus_model ? `Opus ${source.default_opus_model}` : null,
        source.third_party_model ? `第三方 ${source.third_party_model}` : null,
      ].filter(Boolean);
      return parts.length > 0 ? parts.join(" · ") : "—";
    }

    function formatClaudeAccessMode(preset) {
      const mode = preset?.access_mode || (preset?.use_local_proxy ? "anthropic_relay" : "direct");
      switch (mode) {
        case "anthropic_proxy":
        case "anthropic_relay":
          return "Anthropic 无转换中转";
        case "openai_chat":
          return "OpenAI Chat → Anthropic Messages";
        case "openai_responses":
          return "OpenAI Responses → Anthropic Messages";
        default:
          return "不转换";
      }
    }

    function formatCurrentClaudeSummary(currentClaude) {
      return [
        "当前 ~/.claude/settings.json",
        currentClaude.preset_name ? `预设 ${currentClaude.preset_name}` : null,
        currentClaude.provider_name ? `名字 ${currentClaude.provider_name}` : null,
        currentClaude.base_url ? `Base ${currentClaude.base_url}` : null,
        currentClaude.management_url ? `管理 ${currentClaude.management_url}` : null,
        currentClaude.default_haiku_model ? `Haiku ${currentClaude.default_haiku_model}` : null,
        currentClaude.default_sonnet_model ? `Sonnet ${currentClaude.default_sonnet_model}` : null,
        currentClaude.default_opus_model ? `Opus ${currentClaude.default_opus_model}` : null,
        currentClaude.third_party_model ? `第三方 ${currentClaude.third_party_model}` : null,
        currentClaude.masked_auth_token ? `Token ${currentClaude.masked_auth_token}` : null,
      ].filter(Boolean).join(" · ");
    }

    function formatCurrentClaudeStatus(response) {
      if (response.current_claude) {
        return formatCurrentClaudeSummary(response.current_claude);
      }

      return response.current_settings_error || "当前尚未配置 Claude Code。";
    }

    function claudeStatusSortRank(preset) {
      if (preset?.active) {
        return 3;
      }
      if (state.claudePresetsTesting.has(preset?.id)) {
        return 2;
      }
      const result = state.claudePresetTestResults.get(preset?.id);
      if (result) {
        return result.ok ? 1 : -1;
      }
      return 0;
    }

    function buildClaudePresetSortColumns(configKeys) {
      return [
        { key: "status", type: "number", defaultDirection: "desc", getValue: claudeStatusSortRank },
        { key: "name", type: "text", getValue: (preset) => preset?.name || "" },
        { key: "base_url", type: "text", getValue: (preset) => preset?.base_url || "" },
        { key: "management_url", type: "text", getValue: (preset) => preset?.management_url || "" },
        { key: "token", type: "text", getValue: (preset) => preset?.masked_auth_token || "" },
        { key: "models", type: "text", getValue: formatClaudeModelSummary },
        { key: "access_mode", type: "text", getValue: formatClaudeAccessMode },
        { key: "switch_count", type: "number", defaultDirection: "desc", getValue: (preset) => preset?.switch_count || 0 },
        ...createPresetConfigSortColumns(configKeys, "settings.json env: "),
        { key: "saved_at", type: "date", defaultDirection: "desc", getValue: (preset) => preset?.saved_at || "" },
      ];
    }

    async function moveClaudePresetOrder(presetId, direction) {
      await movePresetOrderWithPersist({
        presets: state.claudePresets,
        presetId,
        direction,
        sortTableKey: "claude",
        reorderUrl: "/api/auth/claude-presets/reorder",
        label: "Claude",
        renderFn: renderClaudePresets,
        getStatus: () => state.claudePresets,
        setStatus: (p) => { state.claudePresets = p; },
        persistOrder: persistPresetOrder,
        updateStatus: (msg, tone) => setInlineStatus(claudeManagerStatusEl, msg, tone),
      });
    }

    function renderClaudePresets(presets) {
      const configKeys = collectPresetConfigKeys(presets);
      const sortColumns = buildClaudePresetSortColumns(configKeys);
      const validIds = new Set(presets.map((preset) => preset.id));
      state.claudePresetExportSelection.forEach((id) => {
        if (!validIds.has(id)) state.claudePresetExportSelection.delete(id);
      });
      const selection = {
        label: "Claude_API 账号",
        presets,
        selectedIds: state.claudePresetExportSelection,
        onChange: () => renderClaudePresets(presets),
      };
      renderClaudePresetTableHeader(configKeys, {
        sortColumns,
        onSortChange: () => renderClaudePresets(presets),
        selection,
      });

      renderPresetTable({
        listEl: claudePresetListEl,
        presets,
        emptyText: "还没有保存任何 Claude 预设。",
        emptyColspan: 16 + configKeys.length,
        group: {
          getKey: (preset) => preset?.base_url || "",
          normalizeKey: normalizePresetBaseUrlGroupKey,
          mergeCellKey: "base_url",
        },
        tableKey: "claude",
        sortColumns,
        order: {
          enabled: true,
          onMove: moveClaudePresetOrder,
        },
        selection,
        buildCells: (preset) => {
          const currentCell = createCurrentIndicatorCell(preset.active, {
            testResult: state.claudePresetTestResults.get(preset.id) || null,
            testKind: "claude",
            testing: state.claudePresetsTesting.has(preset.id),
          });
          const nameCell = createPresetNameCell(preset.name);
          const baseUrlCell = createTextCell(preset.base_url, "mono-text");
          baseUrlCell.dataset.presetColumn = "base_url";

          const managementUrlCell = createExternalUrlCell(preset.management_url);

          const tokenCell = createTextCell(preset.masked_auth_token, "mono-text");

          const modelsCell = createTextCell(formatClaudeModelSummary(preset), "mono-text");
          const accessModeCell = createTextCell(
            formatClaudeAccessMode(preset),
            "mono-text",
          );
          const switchCountCell = createTextCell(String(preset.switch_count || 0), "mono-text");

          const configValues = buildPresetConfigValueMap(preset);
          const configCells = buildPresetConfigCells(configKeys, configValues);

          const timeCell = createTextCell(formatDateTime(preset.saved_at));

          const applyCell = createActionCell(
            [createActionButton("切换", () => applyClaudePreset(preset.id), "mini-button accent")],
            "auth-action-cell",
            "actions preset-actions",
          );
          const testCell = createActionCell(
            [createActionButton("测试", () => testClaudePreset(preset.id, preset.name), "mini-button")],
            "auth-action-cell",
            "actions preset-actions",
          );
          const opencodeCell = createActionCell(
            [createActionButton("OpenCode", () => applyClaudePresetToOpencode(preset.id), "mini-button")],
            "auth-action-cell",
            "actions preset-actions",
          );
          const editCell = createActionCell(
            [createActionButton("编辑", () => editClaudePreset(preset.id), "mini-button")],
            "auth-action-cell",
            "actions preset-actions",
          );
          const deleteCell = createActionCell(
            [createPresetDeleteButton(() => deleteClaudePreset(preset.id, preset.name))],
            "auth-action-cell",
            "actions preset-actions",
          );

          return [
            applyCell,
            testCell,
            opencodeCell,
            editCell,
            deleteCell,
            currentCell,
            nameCell,
            baseUrlCell,
            managementUrlCell,
            tokenCell,
            modelsCell,
            accessModeCell,
            switchCountCell,
            ...configCells,
            timeCell,
          ];
        },
      });
    }

    async function loadClaudePresets() {
      if (state.claudePresetsLoading) {
        return;
      }

      state.claudePresetsLoading = true;
      setInlineStatus(claudeManagerStatusEl, "", "info", true);
      if (claudeTestAllPresetsButton) {
        claudeTestAllPresetsButton.disabled = true;
      }

      try {
        const response = await requestJson("/api/auth/claude-presets");
        state.claudePresets = response.presets;
        prunePresetTestResults(state.claudePresetTestResults, response.presets, state.claudePresetsTesting);
        state.upstreamProxy = normalizeUpstreamProxySettings(response.upstream_proxy);
        renderUpstreamProxyToggles();
        state.claudePresetsLoaded = true;
        if (
          state.editingClaudePresetId &&
          !response.presets.some((preset) => preset.id === state.editingClaudePresetId)
        ) {
          resetClaudePresetForm("正在编辑的 Claude 预设已不存在。", "warn");
        }
        claudeCurrentFileEl.textContent = response.settings_file;
        claudePresetFileEl.textContent = response.preset_file;
        claudeCurrentTargetEl.textContent = formatCurrentClaudeStatus(response);
        renderClaudePresets(response.presets);
        renderClaudeModelOptions();
        refreshBaseUrlPresetOptions();
        refreshConfigOverrideDatalists();
        if (claudeTestAllPresetsButton) {
          claudeTestAllPresetsButton.disabled = response.presets.length === 0;
        }
        setInlineStatus(claudeManagerStatusEl, "", "ok", true);
      } catch (error) {
        state.claudePresets = [];
        state.claudePresetsLoaded = false;
        claudePresetListEl.textContent = "";
        refreshBaseUrlPresetOptions();
        claudeCurrentFileEl.textContent = "读取失败";
        claudeCurrentTargetEl.textContent = "读取失败";
        claudePresetFileEl.textContent = "读取失败";
        renderClaudeModelOptions();
        refreshConfigOverrideDatalists();
        if (claudeTestAllPresetsButton) {
          claudeTestAllPresetsButton.disabled = true;
        }
        setInlineStatus(claudeManagerStatusEl, error.message, "warn", false);
      } finally {
        state.claudePresetsLoading = false;
      }
    }

    function ensureClaudePresetsLoaded() {
      if (state.claudePresetsLoaded || state.claudePresetsLoading) {
        return;
      }

      loadClaudePresets();
    }

    function setClaudePresetEditingState(presetId = "") {
      state.editingClaudePresetId = presetId;
      claudeSavePresetButton.textContent = presetId ? "编辑" : "新增";
      claudeSaveAsNewPresetButton.disabled = !presetId;
      claudeSaveAsNewPresetButton.hidden = !presetId;
      claudeSaveAsNewPresetButton.textContent = "新增";
      claudeApplyEditedPresetButton.disabled = !presetId;
      claudeClearInputButton.textContent = presetId ? "取消编辑" : "清空";
    }

    function getSelectedClaudeModelMode() {
      if (claudeModelModeThirdPartyInputEl.checked) {
        return "third-party";
      }
      if (claudeModelModeOfficialInputEl.checked) {
        return "official";
      }
      return "";
    }

    function sourceHasClaudeOfficialModels(source) {
      return Boolean(source.default_haiku_model || source.default_sonnet_model || source.default_opus_model);
    }

    function preferredClaudeModelModeFromSource(source) {
      if (sourceHasClaudeOfficialModels(source)) {
        return "official";
      }
      return source.third_party_model ? "third-party" : "";
    }

    function setClaudeModelGroupAppearance(groupEl, { active = false, inactive = false } = {}) {
      if (!groupEl) {
        return;
      }

      groupEl.classList.toggle("is-active", active);
      groupEl.classList.toggle("is-inactive", inactive);
    }

    function setClaudeModelGroupEnabled(groupEl, enabled) {
      if (!groupEl) {
        return;
      }

      groupEl.setAttribute("aria-disabled", enabled ? "false" : "true");
      groupEl.querySelectorAll("input, button").forEach((control) => {
        if (control.name === "claude-model-mode") {
          return;
        }
        control.disabled = !enabled;
      });
    }

    function syncClaudeModelGroupState(preferredMode = "") {
      const mode = preferredMode || getSelectedClaudeModelMode() || "official";

      claudeModelModeOfficialInputEl.checked = mode === "official";
      claudeModelModeThirdPartyInputEl.checked = mode === "third-party";

      setClaudeModelGroupAppearance(claudeOfficialModelGroupEl, {
        active: mode === "official",
        inactive: mode === "third-party",
      });
      setClaudeModelGroupAppearance(claudeThirdPartyModelGroupEl, {
        active: mode === "third-party",
        inactive: mode === "official",
      });
      setClaudeModelGroupEnabled(claudeOfficialModelGroupEl, mode === "official");
      setClaudeModelGroupEnabled(claudeThirdPartyModelGroupEl, mode === "third-party");

      return {
        mode,
      };
    }

    function validateClaudeModelSelection() {
      if (!getSelectedClaudeModelMode()) {
        updateStatus(claudeFormStatusEl, "先选择官方模型设置或第三方模型设置。", "warn");
        return false;
      }
      return true;
    }

    function resetClaudePresetForm(message = "输入内容已清空。", tone = "muted") {
      claudeProviderNameInputEl.value = "";
      claudeAuthTokenInputEl.value = "";
      claudeBaseUrlInputEl.value = "";
      claudeManagementUrlInputEl.value = "";
      claudeDefaultHaikuModelInputEl.value = "";
      claudeDefaultSonnetModelInputEl.value = "";
      claudeDefaultOpusModelInputEl.value = "";
      claudeThirdPartyModelInputEl.value = "";
      if (claudeAccessModeInputEl) {
        claudeAccessModeInputEl.value = "direct";
      }
      renderConfigOverrideEditor(claudeConfigOverrideControls, [], { open: false });
      syncClaudeModelGroupState("official");
      setClaudePresetEditingState("");
      updateStatus(claudeFormStatusEl, message || CLAUDE_FORM_DEFAULT_STATUS, tone);
    }

    function editClaudePreset(presetId) {
      const preset = state.claudePresets.find((item) => item.id === presetId);
      if (!preset) {
        updateStatus(claudeManagerStatusEl, "找不到要编辑的 Claude 预设。", "warn");
        return;
      }

      claudeProviderNameInputEl.value = preset.provider_name ?? "";
      claudeAuthTokenInputEl.value = preset.auth_token ?? "";
      claudeBaseUrlInputEl.value = preset.base_url;
      claudeManagementUrlInputEl.value = preset.management_url ?? "";
      claudeDefaultHaikuModelInputEl.value = preset.default_haiku_model ?? "";
      claudeDefaultSonnetModelInputEl.value = preset.default_sonnet_model ?? "";
      claudeDefaultOpusModelInputEl.value = preset.default_opus_model ?? "";
      claudeThirdPartyModelInputEl.value = preset.third_party_model ?? "";
      if (claudeAccessModeInputEl) {
        const accessMode = preset.access_mode || (preset.use_local_proxy ? "anthropic_relay" : "direct");
        claudeAccessModeInputEl.value = accessMode === "anthropic_proxy" ? "anthropic_relay" : accessMode;
      }
      const configOverrides = normalizePresetConfigOverrides(preset);
      renderConfigOverrideEditor(claudeConfigOverrideControls, configOverrides, {
        open: configOverrides.length > 0,
      });
      const preferredMode = preferredClaudeModelModeFromSource(preset) || "official";
      syncClaudeModelGroupState(preferredMode);
      setClaudePresetEditingState(preset.id);
      claudeProviderNameInputEl.focus();
      claudeProviderNameInputEl.select();
      claudeProviderNameInputEl.scrollIntoView({ behavior: "smooth", block: "nearest" });
      if (sourceHasClaudeOfficialModels(preset) && preset.third_party_model) {
        updateStatus(
          claudeFormStatusEl,
          `正在编辑 Claude 预设：${preset.name}。该预设同时含有两组模型设置，界面已默认选中官方模型设置，保存后会按当前单选项修正。`,
          "warn",
        );
      } else {
        updateStatus(claudeFormStatusEl, `正在编辑 Claude 预设：${preset.name}`, "info");
      }
    }

    async function saveClaudePreset() {
      return saveClaudePresetWithMode(false);
    }

    async function saveClaudePresetAsNew() {
      return saveClaudePresetWithMode(true);
    }

    async function saveClaudePresetWithMode(forceNewPreset = false) {
      const editingPresetId = state.editingClaudePresetId;
      const modelMode = getSelectedClaudeModelMode() || "official";
      const name = claudeProviderNameInputEl.value.trim();
      const providerName = claudeProviderNameInputEl.value.trim();
      const authToken = claudeAuthTokenInputEl.value.trim();
      const baseUrl = claudeBaseUrlInputEl.value.trim();
      const managementUrl = claudeManagementUrlInputEl.value.trim();
      const defaultHaikuModel =
        modelMode === "official" ? claudeDefaultHaikuModelInputEl.value.trim() : "";
      const defaultSonnetModel =
        modelMode === "official" ? claudeDefaultSonnetModelInputEl.value.trim() : "";
      const defaultOpusModel =
        modelMode === "official" ? claudeDefaultOpusModelInputEl.value.trim() : "";
      const thirdPartyModel =
        modelMode === "third-party" ? claudeThirdPartyModelInputEl.value.trim() : "";
      const configOverrides = collectConfigOverrideEditorValues(claudeConfigOverrideControls);

      if (!authToken) {
        updateStatus(claudeFormStatusEl, "先填写 Auth Token。", "warn");
        return;
      }
      if (!baseUrl) {
        updateStatus(claudeFormStatusEl, "先填写 Base URL。", "warn");
        return;
      }
      if (!validateClaudeModelSelection()) {
        return;
      }

      const editingPreset = state.claudePresets.find((item) => item.id === editingPresetId);
      const isEditing = Boolean(editingPresetId) && !forceNewPreset;
      if (isEditing && !confirmEditedPresetOverwrite({
        presetKind: "Claude 预设",
        presetName: editingPreset?.name || name,
      })) {
        return;
      }

      updateStatus(
        claudeFormStatusEl,
        isEditing ? "正在更新 Claude 预设…" : "正在新增 Claude 预设…",
        "info",
      );
      claudeSavePresetButton.disabled = true;
      claudeSaveAsNewPresetButton.disabled = true;
      claudeApplyEditedPresetButton.disabled = true;
      claudeClearInputButton.disabled = true;

      try {
        const response = await requestJson(
          isEditing
            ? `/api/auth/claude-presets/${encodeURIComponent(editingPresetId)}`
            : "/api/auth/claude-presets",
          {
            method: isEditing ? "PUT" : "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify({
              name,
              provider_name: providerName,
              auth_token: authToken,
              base_url: baseUrl,
              management_url: managementUrl || null,
              default_haiku_model: defaultHaikuModel || null,
              default_sonnet_model: defaultSonnetModel || null,
              default_opus_model: defaultOpusModel || null,
              third_party_model: thirdPartyModel || null,
              access_mode: claudeAccessModeInputEl?.value || "direct",
              use_local_proxy: ["anthropic_relay", "openai_chat", "openai_responses"].includes(
                claudeAccessModeInputEl?.value,
              ),
              config_overrides: configOverrides,
            }),
          },
        );
        resetClaudePresetForm(
          `${isEditing ? "已更新" : "已新增"}预设：${response.preset.name}`,
          "ok",
        );
        await loadClaudePresets();
      } catch (error) {
        updateStatus(claudeFormStatusEl, error.message, "warn");
      } finally {
        claudeSavePresetButton.disabled = false;
        claudeSaveAsNewPresetButton.disabled = false;
        claudeApplyEditedPresetButton.disabled = false;
        claudeClearInputButton.disabled = false;
        setClaudePresetEditingState(state.editingClaudePresetId);
      }
    }

    async function applyClaudePreset(presetId, statusElement = claudeManagerStatusEl) {
      updateStatus(statusElement, "正在切换 Claude 预设…", "info");

      try {
        const response = await requestJson(`/api/auth/claude-presets/${encodeURIComponent(presetId)}/apply`, {
          method: "PUT",
        });
        updateStatus(
          statusElement,
          response.deferred
            ? `已登记 Claude 预设切换：${response.name}。当前指定 Agent 退出并恢复原配置后，将实际写入该预设。`
            : `已应用 Claude 预设：${response.name}`,
          response.deferred ? "info" : "ok",
        );
        await refreshAuthPanels();
      } catch (error) {
        updateStatus(statusElement, error.message, "warn");
      }
    }

    async function applyEditingClaudePreset() {
      if (!state.editingClaudePresetId) {
        updateStatus(claudeFormStatusEl, "先点击一条 Claude 预设的编辑。", "warn");
        return;
      }

      await applyClaudePreset(state.editingClaudePresetId, claudeFormStatusEl);
    }

    async function applyClaudePresetToOpencode(presetId, statusElement = claudeManagerStatusEl) {
      updateStatus(statusElement, "正在应用到 opencode…", "info");

      try {
        const response = await requestJson(`/api/auth/claude-presets/${encodeURIComponent(presetId)}/apply-opencode`, {
          method: "PUT",
        });
        updateStatus(statusElement, `已应用到 opencode：${response.name}（${response.settings_file}）`, "ok");
      } catch (error) {
        updateStatus(statusElement, error.message, "warn");
      }
    }

    async function testClaudePreset(presetId, presetName) {
      state.claudePresetsTesting.add(presetId);
      presetTestPopup.hide();
      renderClaudePresets(state.claudePresets);
      try {
        const response = await requestJson(`/api/auth/claude-presets/${encodeURIComponent(presetId)}/test`, {
          method: "POST",
        });
        const result = normalizeTestResult(response.result, {
          presetId,
          fallbackName: presetName,
        });
        state.claudePresetTestResults.set(presetId, result);
      } catch (error) {
        state.claudePresetTestResults.set(
          presetId,
          errorTestResult(error, { presetId, fallbackName: presetName }),
        );
      } finally {
        state.claudePresetsTesting.delete(presetId);
        presetTestPopup.hide();
        renderClaudePresets(state.claudePresets);
      }
    }

    async function testAllClaudePresets() {
      if (!state.claudePresets.length) {
        updateStatus(claudeManagerStatusEl, "没有可测试的 Claude 预设。", "warn");
        return;
      }

      state.claudePresets.forEach((preset) => state.claudePresetsTesting.add(preset.id));
      setButtonBusy(claudeTestAllPresetsButton, true, "测试中…");
      presetTestPopup.hide();
      renderClaudePresets(state.claudePresets);
      try {
        const response = await requestJson("/api/auth/claude-presets/test-all", {
          method: "POST",
        });
        const results = Array.isArray(response.results) ? response.results : [];
        results.forEach((raw) => {
          const normalized = normalizeTestResult(raw, { presetId: raw.preset_id });
          state.claudePresetTestResults.set(normalized.preset_id, normalized);
        });
      } catch (error) {
        state.claudePresets.forEach((preset) => {
          state.claudePresetTestResults.set(
            preset.id,
            errorTestResult(error, { presetId: preset.id, fallbackName: preset.name }),
          );
        });
      } finally {
        state.claudePresetsTesting.clear();
        setButtonBusy(claudeTestAllPresetsButton, false);
        presetTestPopup.hide();
        renderClaudePresets(state.claudePresets);
        if (claudeTestAllPresetsButton) {
          claudeTestAllPresetsButton.disabled = state.claudePresets.length === 0;
        }
      }
    }

    async function deleteClaudePreset(presetId, presetName) {
      if (!window.confirm(`确定删除 Claude 预设"${presetName}"吗？`)) {
        return;
      }

      updateStatus(claudeManagerStatusEl, "正在删除 Claude 预设…", "info");

      try {
        await requestJson(`/api/auth/claude-presets/${encodeURIComponent(presetId)}`, {
          method: "DELETE",
        });
        if (state.editingClaudePresetId === presetId) {
          resetClaudePresetForm("正在编辑的 Claude 预设已删除。", "warn");
        }
        updateStatus(claudeManagerStatusEl, `已删除预设：${presetName}`, "ok");
        await loadClaudePresets();
      } catch (error) {
        updateStatus(claudeManagerStatusEl, error.message, "warn");
      }
    }

    return {
      applyClaudePreset,
      applyClaudePresetToOpencode,
      applyEditingClaudePreset,
      deleteClaudePreset,
      editClaudePreset,
      ensureClaudePresetsLoaded,
      formatCurrentClaudeStatus,
      loadClaudePresets,
      normalizeClaudeModelOptions,
      parseClaudeModelOptionsInput,
      renderClaudeModelOptions,
      renderClaudePresets,
      resetClaudePresetForm,
      saveClaudePreset,
      saveClaudePresetAsNew,
      syncClaudeModelGroupState,
      testAllClaudePresets,
      testClaudePreset,
    };
  }

  globalThis.WebClxClaudeManager = Object.freeze({ create: createClaudeManager });
})();
