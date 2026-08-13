// Preset and settings form event bindings for Codex/API/Auth/Claude panels.
// Called by app.js after managers and DOM globals are initialized.

function bindPresetFormEventHandlers() {
  codexApiAutoProxyProviderInputEls.forEach((input) => {
    input.addEventListener("change", () => {
      state.codexApiAutoProxyMatchProviderIds = readCodexApiAutoProxyMatchProviderIds();
      state.apiApplyProxyManuallyChanged = false;
      syncApiApplyProxyRecommendation();
    });
  });

  workspaceDirInputEl.addEventListener("focus", () => {
    workspaceDirInputEl.scrollLeft = 0;
  });

  if (terminalQuickCommandAddButtonEl) {
    terminalQuickCommandAddButtonEl.addEventListener("click", addTerminalQuickCommandRow);
  }

  if (terminalQuickCommandFormEl) {
    terminalQuickCommandFormEl.addEventListener("submit", (event) => {
      event.preventDefault();
      commitTerminalQuickCommandEditor({ silent: false, allowEmpty: false });
    });
  }

  if (terminalQuickStartDefaultSelectEl) {
    terminalQuickStartDefaultSelectEl.addEventListener("change", () => {
      renderTerminalQuickCommands(
        readTerminalQuickCommandsFromInputs(),
        terminalQuickStartDefaultSelectEl.value || "",
      );
      const defaultCommand = state.terminalQuickCommands.find(
        (command) => command.key === state.terminalQuickStartDefaultKey,
      );
      updateStatus(
        settingsStatusEl,
        defaultCommand
          ? `默认启动已设为 ${defaultCommand.key} - ${terminalQuickCommandDisplay(defaultCommand)}；点击“保存设置”后才会持久化。`
          : "已设为不自动启动；点击“保存设置”后才会持久化。",
        "muted",
      );
    });
  }

  if (terminalQuickCommandCancelButtonEl) {
    terminalQuickCommandCancelButtonEl.addEventListener("click", () => {
      clearTerminalQuickCommandEditor();
    });
  }

  if (terminalShortcutsListEl) {
    terminalShortcutsListEl.addEventListener("input", (event) => {
      const input = event.target?.closest?.(".terminal-shortcut-input");
      if (!input) {
        return;
      }
      updateTerminalShortcutCommand(
        input.dataset.commandGroup || "",
        Number(input.dataset.commandIndex),
        input.value,
      );
    });
  }

  if (terminalShortcutsResetButtonEl) {
    terminalShortcutsResetButtonEl.addEventListener("click", resetTerminalShortcutsToDefaults);
  }

  if (terminalCommandCollectionAddBtnEl) {
    terminalCommandCollectionAddBtnEl.addEventListener("click", addTerminalCommandCollection);
  }

  terminalSlashCommandsInputEl?.addEventListener("change", syncTerminalCommandStateFromTextareas);
  terminalFunctionCommandsInputEl?.addEventListener("change", syncTerminalCommandStateFromTextareas);

  if (terminalQuickCommandsListEl) {
    terminalQuickCommandsListEl.addEventListener("click", (event) => {
      const button = event.target?.closest?.("[data-action]");
      if (!button) {
        return;
      }

      const index = Number(button.dataset.index);
      if (!Number.isInteger(index) || index < 0) {
        return;
      }

      if (button.dataset.action === "edit-terminal-quick-command") {
        editTerminalQuickCommand(index);
        return;
      }

      if (button.dataset.action === "set-terminal-quick-default") {
        const quickKey = String(button.dataset.quickKey || "").trim();
        renderTerminalQuickCommands(readTerminalQuickCommandsFromInputs(), quickKey);
        const defaultCommand = state.terminalQuickCommands.find(
          (command) => command.key === state.terminalQuickStartDefaultKey,
        );
        updateStatus(
          settingsStatusEl,
          defaultCommand
            ? `默认启动已设为 ${defaultCommand.key} - ${terminalQuickCommandDisplay(defaultCommand)}；点击“保存设置”后才会持久化。`
            : "已设为不自动启动；点击“保存设置”后才会持久化。",
          "muted",
        );
        return;
      }

      if (button.dataset.action === "delete-terminal-quick-command") {
        const commands = readTerminalQuickCommandsFromInputs();
        commands.splice(index, 1);
        clearTerminalQuickCommandEditor();
        renderTerminalQuickCommands(commands, terminalQuickStartDefaultSelectEl?.value || "");
        updateStatus(settingsStatusEl, "快捷命令已移除；点击“保存设置”后才会持久化。", "muted");
      }
    });
  }

  [
    fontSizeTier1InputEl,
    fontSizeTier2InputEl,
    fontSizeTier3InputEl,
    fontSizeTier4InputEl,
  ]
    .filter(Boolean)
    .forEach((input) => {
      input.addEventListener("input", () => {
        applyTypographySettings(readFontSizeTiersFromInputs());
        updateFontSettingsSummary();
        updateStatus(settingsStatusEl, "字体预览已更新；点击“保存设置”后才会持久化。", "muted");
      });
    });

  if (terminalFloatingButtonOffsetInputEl) {
    terminalFloatingButtonOffsetInputEl.addEventListener("change", () => {
      terminalFloatingButtonOffsetInputEl.value = formatTerminalFloatingButtonOffsetVh(
        readTerminalFloatingButtonOffsetVhFromInput(),
      );
      updateStatus(settingsStatusEl, "终端悬浮按钮高度已更新；点击“保存设置”后才会持久化。", "muted");
    });
  }

  if (terminalFabActionColorInputEl) {
    terminalFabActionColorInputEl.addEventListener("input", () => {
      terminalFabActionColorInputEl.value = readTerminalFabActionColorFromInput();
      document.documentElement.style.setProperty(
        "--terminal-fab-action-color",
        terminalFabActionColorInputEl.value,
      );
      updateStatus(settingsStatusEl, "终端侧边按钮颜色预览已更新；点击“保存设置”后持久化。", "muted");
    });
  }

  if (terminalFabActionOpacityInputEl) {
    terminalFabActionOpacityInputEl.addEventListener("input", () => {
      const opacity = readTerminalFabActionOpacityFromInput();
      terminalFabActionOpacityInputEl.value = String(opacity);
      renderTerminalFabActionOpacityOutput(opacity);
      document.documentElement.style.setProperty("--terminal-fab-action-opacity", String(opacity));
      updateStatus(settingsStatusEl, "终端侧边按钮透明度预览已更新；点击“保存设置”后持久化。", "muted");
    });
  }

  if (terminalSoftKeyboardScaleInputEl) {
    terminalSoftKeyboardScaleInputEl.addEventListener("change", () => {
      terminalSoftKeyboardScaleInputEl.value = formatTerminalSoftKeyboardScale(
        terminalSoftKeyboardScaleInputEl.value,
      );
      updateStatus(settingsStatusEl, "终端软键盘大小已更新；点击“保存设置”后才会持久化。", "muted");
    });
  }

  if (terminalTouchSelectionLongPressInputEl) {
    terminalTouchSelectionLongPressInputEl.addEventListener("change", () => {
      terminalTouchSelectionLongPressInputEl.value = formatTerminalTouchSelectionLongPressMs(
        terminalTouchSelectionLongPressInputEl.value,
      );
      updateStatus(settingsStatusEl, "触摸复制长按时间已更新；点击“保存设置”后才会持久化。", "muted");
    });
  }

  enableQuickSelectInput(apiBaseUrlInputEl);
  enableQuickSelectInput(apiManagementUrlInputEl);
  enableQuickSelectInput(claudeBaseUrlInputEl);
  enableQuickSelectInput(claudeManagementUrlInputEl);

  if (apiBaseUrlInputEl) {
    apiBaseUrlInputEl.addEventListener("input", () => {
      if (apiManagementUrlSameAsBaseInputEl?.checked) {
        syncApiManagementUrlField({
          useBaseUrl: true,
          syncValue: true,
        });
      }
      syncApiApplyProxyRecommendation();
    });
  }

  if (apiPresetNameEl) {
    apiPresetNameEl.addEventListener("input", syncApiApplyProxyRecommendation);
  }

  if (apiConfigOverridesListEl) {
    apiConfigOverridesListEl.addEventListener("input", syncApiApplyProxyRecommendation);
    apiConfigOverridesListEl.addEventListener("change", syncApiApplyProxyRecommendation);
  }

  if (apiResponsesProxyInputEl) {
    apiResponsesProxyInputEl.addEventListener("change", syncApiApplyProxyRecommendation);
  }

  if (apiApplyUpstreamProxyOnSwitchInputEl) {
    apiApplyUpstreamProxyOnSwitchInputEl.addEventListener("change", () => {
      state.apiApplyProxyManuallyChanged = true;
      warnApiApplyProxyRecommendationIfNeeded();
    });
  }

  if (apiManagementUrlSameAsBaseInputEl) {
    apiManagementUrlSameAsBaseInputEl.addEventListener("change", () => {
      syncApiManagementUrlField({
        useBaseUrl: apiManagementUrlSameAsBaseInputEl.checked,
        syncValue: apiManagementUrlSameAsBaseInputEl.checked,
        focusInput: !apiManagementUrlSameAsBaseInputEl.checked,
      });
    });
  }

  syncApiManagementUrlField({
    useBaseUrl: apiManagementUrlSameAsBaseInputEl?.checked ?? true,
    syncValue: true,
  });

  renderConfigOverrideEditor(authConfigOverrideControls, [], { open: false });
  renderConfigOverrideEditor(apiConfigOverrideControls, [], { open: false });
  renderConfigOverrideEditor(claudeConfigOverrideControls, [], { open: false });
  renderAuthOauthSession(null);
  updateAuthOauthStatus("");

  authAddConfigOverrideButton.addEventListener("click", () => {
    addConfigOverrideRow(authConfigOverrideControls);
  });

  apiAddConfigOverrideButton.addEventListener("click", () => {
    addConfigOverrideRow(apiConfigOverrideControls);
  });

  claudeAddConfigOverrideButton.addEventListener("click", () => {
    addConfigOverrideRow(claudeConfigOverrideControls);
  });

  authSavePresetButton.addEventListener("click", () => {
    saveAuthPreset();
  });

  authSaveAsNewPresetButton.addEventListener("click", () => {
    saveAuthPresetAsNew();
  });

  authApplyEditedPresetButton.addEventListener("click", () => {
    applyEditingAuthPreset();
  });

  authClearInputButton.addEventListener("click", () => {
    resetAuthPresetForm(state.editingAuthPresetId ? "已取消编辑。" : "输入内容已清空。", "muted");
  });

  if (authImportFileButton && authImportFileInputEl) {
    authImportFileButton.addEventListener("click", () => {
      authImportFileInputEl.click();
    });
    authImportFileInputEl.addEventListener("change", () => {
      const [sourceFile] = authImportFileInputEl.files || [];
      authImportFileInputEl.value = "";
      importAuthJsonFile(sourceFile);
    });
  }

  if (apiAccountImportFileButton && apiAccountImportFileInputEl) {
    apiAccountImportFileButton.addEventListener("click", () => {
      apiAccountImportFileInputEl.click();
    });
    apiAccountImportFileInputEl.addEventListener("change", () => {
      const sourceFiles = Array.from(apiAccountImportFileInputEl.files || []);
      apiAccountImportFileInputEl.value = "";
      apiManager.importApiAccountsFromFiles(sourceFiles);
    });
  }

  if (apiAccountImportTextButton) {
    apiAccountImportTextButton.addEventListener("click", () => {
      if (apiAccountImportTextEl) apiAccountImportTextEl.value = "";
      if (apiAccountImportStatusEl) {
        updateStatus(apiAccountImportStatusEl, "", "muted");
      }
      if (typeof apiAccountImportDialogEl.showModal === "function") {
        if (!apiAccountImportDialogEl.open) {
          apiAccountImportDialogEl.showModal();
        }
      } else {
        apiAccountImportDialogEl.setAttribute("open", "");
      }
      window.requestAnimationFrame(() => {
        apiAccountImportTextEl?.focus();
      });
    });
  }

  if (apiAccountImportCancelButton) {
    apiAccountImportCancelButton.addEventListener("click", () => {
      if (apiAccountImportDialogEl.open) {
        apiAccountImportDialogEl.close();
      }
    });
  }

  if (apiAccountImportFormEl) {
    apiAccountImportFormEl.addEventListener("submit", (event) => {
      event.preventDefault();
      const rawText = apiAccountImportTextEl?.value || "";
      apiManager.importApiAccountsFromText(rawText);
    });
  }

  if (authOauthStartButton) {
    authOauthStartButton.addEventListener("click", () => {
      startAuthOauthSession();
    });
  }

  if (authOauthCopyCodeButton) {
    authOauthCopyCodeButton.addEventListener("click", copyAuthOauthUserCode);
  }

  authRefreshAllQuotaButton.addEventListener("click", () => {
    refreshAllAuthPresetQuotas();
  });

  if (authTestAllPresetsButton) {
    authTestAllPresetsButton.addEventListener("click", testAllAuthPresets);
  }

  apiSavePresetButton.addEventListener("click", () => {
    saveApiPreset();
  });

  apiSaveAsNewPresetButton.addEventListener("click", () => {
    saveApiPresetAsNew();
  });

  apiApplyEditedPresetButton.addEventListener("click", () => {
    applyEditingApiPreset();
  });

  if (apiAddPresetButton) {
    apiAddPresetButton.addEventListener("click", () => {
      startNewApiPreset();
    });
  }

  apiClearInputButton.addEventListener("click", () => {
    resetApiPresetForm(state.editingApiPresetId ? "已取消编辑。" : "输入内容已清空。", "muted");
  });

  if (apiTestAllPresetsButton) {
    apiTestAllPresetsButton.addEventListener("click", testAllApiPresets);
  }

  claudeSavePresetButton.addEventListener("click", () => {
    saveClaudePreset();
  });

  claudeSaveAsNewPresetButton.addEventListener("click", () => {
    saveClaudePresetAsNew();
  });

  claudeApplyEditedPresetButton.addEventListener("click", () => {
    applyEditingClaudePreset();
  });

  claudeClearInputButton.addEventListener("click", () => {
    resetClaudePresetForm(state.editingClaudePresetId ? "已取消编辑。" : "输入内容已清空。", "muted");
  });

  if (claudeTestAllPresetsButton) {
    claudeTestAllPresetsButton.addEventListener("click", testAllClaudePresets);
  }

  [
    claudeModelModeOfficialInputEl,
    claudeModelModeThirdPartyInputEl,
  ].forEach((input) => {
    input.addEventListener("change", () => {
      if (input.checked) {
        syncClaudeModelGroupState(input.value);
        if (input.value === "official") {
          const noOfficialModels =
            !claudeDefaultHaikuModelInputEl.value.trim() &&
            !claudeDefaultSonnetModelInputEl.value.trim() &&
            !claudeDefaultOpusModelInputEl.value.trim();
          if (noOfficialModels) {
            claudeDefaultHaikuModelInputEl.value = "claude-haiku-4-5-20251001";
            claudeDefaultSonnetModelInputEl.value = "claude-sonnet-4-6";
            claudeDefaultOpusModelInputEl.value = "claude-opus-4-6";
          }
        }
        updateStatus(
          claudeFormStatusEl,
          input.value === "third-party"
            ? "已切换到第三方模型设置，当前只会启用并保存第三方模型这一组。"
            : "已切换到官方模型设置，当前只会启用并保存官方模型这一组。",
          "muted",
        );
      }
    });
  });

  syncClaudeModelGroupState("official");

  authApplyInputButton.addEventListener("click", () => {
    handleAuthApplyInputAction();
  });
}
