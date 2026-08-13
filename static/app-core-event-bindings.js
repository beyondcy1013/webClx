// Core home-page event bindings: editor, workspace, sessions, tabs, and config-file panel.
// Called by app.js after state and DOM globals are initialized.

function bindCoreEventHandlers() {
  bindWorkspaceHistoryPresetForkDialog();
  bindWorkspaceDesignatePresetDialog();

  editorEl.addEventListener("input", () => {
    if (!state.currentFileEditable) {
      return;
    }
    state.dirty = true;
    updateStatus(fileStatusEl, "文件有未保存修改。", "warn");
    updateEditorState();
  });

  editorEl.addEventListener("keydown", (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      saveCurrentFile();
    }
  });

  if (hasDirectorySessionControls) {
    bindSessionSelectInteractionGuard(directorySessionListEl, {
      setBlocked: (blocked) => {
        state.directorySessionUiBlocked = blocked;
      },
      flushPending: () => {
        if (state.pendingDirectorySessionUiSync) {
          syncDirectorySessionControls();
        }
      },
    });

    directorySessionListEl.addEventListener("change", () => {
      if (!directorySessionListEl.value) {
        state.directorySessionId = "";
        return;
      }

      const nextSession = state.directorySessions.find((session) => session.id === directorySessionListEl.value);
      if (!nextSession) {
        state.directorySessionId = "";
        syncDirectorySessionControls();
        return;
      }

      state.directorySessionId = nextSession.id;
      openSession(nextSession);
    });
  }

  refreshSessionsButton.addEventListener("click", () => {
    loadSessions();
  });

  if (sessionsSearchFormEl) {
    sessionsSearchFormEl.addEventListener("submit", (event) => {
      event.preventDefault();
      searchSessionsOutput(sessionsSearchInputEl?.value || "");
    });
  }

  if (sessionsSearchClearButton) {
    sessionsSearchClearButton.addEventListener("click", () => {
      clearSessionsOutputSearch();
      sessionsSearchInputEl?.focus();
    });
  }

  if (refreshTerminalArchivesButton) {
    refreshTerminalArchivesButton.addEventListener("click", () => {
      loadTerminalArchives();
    });
  }

  if (workspaceHistoryRefreshButton) {
    workspaceHistoryRefreshButton.addEventListener("click", () => {
      refreshWorkspaceHistoryConversations();
    });
  }

  if (workspaceHistoryPathSelectEl) {
    workspaceHistoryPathSelectEl.addEventListener("change", () => {
      state.workspaceHistorySelectedPath = workspaceHistoryPathSelectEl.value || "";
      refreshWorkspaceHistoryConversations();
    });
  }

  if (workspaceHistoryTerminalButton) {
    workspaceHistoryTerminalButton.addEventListener("click", () => {
      const selectedPath = workspaceHistorySelectedPath();
      if (selectedPath) {
        openWorkspaceHistoryTerminal(selectedPath);
      }
    });
  }

  if (workspaceHistoryDeleteButton) {
    workspaceHistoryDeleteButton.addEventListener("click", () => {
      const selectedPath = workspaceHistorySelectedPath();
      if (!selectedPath) {
        return;
      }
      persistWorkspaceHistory(state.workspaceHistory.filter((item) => item.path !== selectedPath));
    });
  }

  if (workspaceHistorySearchFormEl) {
    workspaceHistorySearchFormEl.addEventListener("submit", (event) => {
      event.preventDefault();
      const value = workspaceHistorySearchInputEl ? workspaceHistorySearchInputEl.value : "";
      scheduleWorkspaceHistorySearch(value, true);
    });
  }
  if (workspaceHistorySearchClearButton) {
    workspaceHistorySearchClearButton.addEventListener("click", () => {
      clearWorkspaceHistorySearch();
    });
  }
  if (workspaceHistorySearchInputEl) {
    workspaceHistorySearchInputEl.addEventListener("input", (event) => {
      scheduleWorkspaceHistorySearch(event.target.value || "");
    });
  }
  if (workspaceHistorySearchAllEl) {
    workspaceHistorySearchAllEl.addEventListener("change", (event) => {
      state.workspaceHistorySearchAllWorkspaces = Boolean(event.target.checked);
      refreshWorkspaceHistoryConversations();
    });
  }
  if (workspaceHistoryRecentOnlyEl) {
    workspaceHistoryRecentOnlyEl.addEventListener("change", (event) => {
      state.workspaceHistoryRecentOnly = Boolean(event.target.checked);
      renderWorkspaceHistory();
    });
  }

  createSessionButton.addEventListener("click", () => {
    createSession();
  });

  if (hasSessionsSessionControls) {
    bindSessionSelectInteractionGuard(sessionsSessionListEl, {
      setBlocked: (blocked) => {
        state.sessionsSessionUiBlocked = blocked;
      },
      flushPending: () => {
        if (state.pendingSessionsSessionUiSync) {
          renderSessionsSessionPicker();
        }
      },
    });

    sessionsSessionListEl.addEventListener("change", async () => {
      const nextSession = state.sessions.find((session) => session.id === sessionsSessionListEl.value);
      if (!nextSession) {
        state.preferredSessionId = "";
        state.returnTerminalSessionId = "";
        storeGlobalSessionId("");
        syncSessionsTerminalLink();
        syncTabUrl();
        return;
      }

      rememberPreferredSession(nextSession.path, nextSession.id);
      state.directorySessionId = nextSession.id;
      if (!sessionMatchesPath(nextSession)) {
        await navigateTo(nextSession.path);
        return;
      }

      syncDirectorySessionControls();
      syncSessionsTerminalLink();
      syncTabUrl();
    });
  }

  if (sessionRenameDialogEl) {
    sessionRenameDialogEl.addEventListener("cancel", (event) => {
      event.preventDefault();
      closeTerminalRenameDialog();
    });
    sessionRenameDialogEl.addEventListener("click", (event) => {
      if (event.target === sessionRenameDialogEl) {
        closeTerminalRenameDialog();
      }
    });
  }

  if (sessionRenameCancelButton) {
    sessionRenameCancelButton.addEventListener("click", () => {
      closeTerminalRenameDialog();
    });
  }

  if (sessionRenameFormEl) {
    sessionRenameFormEl.addEventListener("submit", (event) => {
      event.preventDefault();
      if (workspaceHistoryRenamingItem) {
        renameWorkspaceHistoryTerminal();
      } else {
        renameSession();
      }
    });
  }

  if (workspaceShowHiddenInputEl) {
    workspaceShowHiddenInputEl.addEventListener("change", (event) => {
      persistWorkspaceShowHidden(Boolean(event.target.checked));
    });
  }

  favoritePathSelectEl.addEventListener("change", () => {
    if (!favoritePathSelectEl.value) {
      return;
    }
    openFavoritePath(favoritePathSelectEl.value);
  });

  currentPathCopyButton?.addEventListener("click", () => {
    copyCurrentPath(currentPathCopyButton);
  });

  terminalLink.addEventListener("click", (event) => {
    recordWorkspaceHistory(state.currentPath);
    openFreshTerminalLink(event, state.currentPath);
  });

  sessionTerminalLink.addEventListener("click", () => {
    recordWorkspaceHistory(state.currentPath);
  });

  saveButton.addEventListener("click", () => {
    saveCurrentFile();
  });

  importAuthButton.addEventListener("click", () => {
    importAuthFromClipboard();
  });

  authImportCancelButton.addEventListener("click", () => {
    closeAuthImportDialog();
  });

  authImportTextEl.addEventListener("paste", () => {
    window.requestAnimationFrame(() => {
      tryAutoApplyAuthImportFromDialog();
    });
  });

  authImportFormEl.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!tryAutoApplyAuthImportFromDialog()) {
      authImportTextEl.focus();
    }
  });

  tabButtons.forEach((button) => {
    button.addEventListener("click", () => {
      setActiveTab(button.dataset.tab);
    });
  });

  settingsCategoryButtons.forEach((button) => {
    button.addEventListener("click", () => {
      setActiveSettingsTab(defaultSettingsTabForCategory(button.dataset.settingsCategory));
    });
  });

  autoContinueTaskRefreshButtonEl?.addEventListener("click", () => {
    loadAutoContinueTasks();
  });

  autoContinueHistoryToggleEl?.addEventListener("click", () => {
    if (!autoContinueHistoryWrapEl) {
      return;
    }
    const willShow = autoContinueHistoryWrapEl.hasAttribute("hidden");
    autoContinueHistoryWrapEl.hidden = !willShow;
    if (autoContinueHistoryToggleEl) {
      autoContinueHistoryToggleEl.textContent = willShow ? "收起历史" : "查看历史";
      autoContinueHistoryToggleEl.setAttribute("aria-expanded", willShow ? "true" : "false");
    }
  });

  autoContinueHistoryClearEl?.addEventListener("click", async () => {
    if (!window.confirm("确定清空全部自动继续历史？此操作不可撤销。")) {
      return;
    }
    setButtonBusy(autoContinueHistoryClearEl, true, "清空中...");
    try {
      const result = await requestJson("/api/terminal/auto-continue-tasks", {
        method: "DELETE",
      });
      const removed = Number(result?.removed || 0);
      await loadAutoContinueTasks();
      if (typeof loadUnifiedTasks === "function") {
        loadUnifiedTasks();
      }
      setUnifiedListStatus?.(
        removed ? `已清空 ${removed} 条历史。` : "历史已是空的。",
        "ok",
      );
    } catch (error) {
      setInlineStatus(autoContinueTaskStatusEl, error.message || "清空历史失败。", "warn");
    } finally {
      setButtonBusy(autoContinueHistoryClearEl, false);
    }
  });

  pasteScheduledTaskRefreshButtonEl?.addEventListener("click", () => {
    loadPasteScheduledTasks();
  });

  pasteScheduledTaskListEl?.addEventListener("click", (event) => {
    const button = event.target.closest?.(".paste-scheduled-cancel");
    if (!button) {
      return;
    }
    cancelPasteScheduledTask(button.dataset.pasteTaskId || "");
  });

  settingsConfigFileSelectEl?.addEventListener("change", () => {
    const nextKey = settingsConfigFileSelectEl.value;
    if (
      state.settingsConfigFileDirty &&
      !window.confirm("当前配置文件有未保存修改。确定切换到其他配置文件吗？")
    ) {
      settingsConfigFileSelectEl.value = state.settingsConfigFileKey || "codex_config";
      return;
    }
    loadSettingsConfigFile(nextKey);
  });

  settingsConfigFileRefreshButtonEl?.addEventListener("click", () => {
    if (
      state.settingsConfigFileDirty &&
      !window.confirm("当前配置文件有未保存修改。确定重新读取并覆盖编辑区吗？")
    ) {
      return;
    }
    loadSettingsConfigFile(settingsConfigFileSelectEl?.value || state.settingsConfigFileKey);
  });

  settingsConfigFileSaveButtonEl?.addEventListener("click", saveSettingsConfigFile);

  settingsConfigFileEditorEl?.addEventListener("input", () => {
    state.settingsConfigFileDirty = true;
    updateStatus(settingsConfigFileStatusEl, "配置文件有未保存修改。", "warn");
  });

  settingsConfigFileEditorEl?.addEventListener("keydown", (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      saveSettingsConfigFile();
    }
  });

  if (themeModeSelectEl) {
    themeModeSelectEl.addEventListener("change", () => {
      const nextThemeMode = applyThemeMode(themeModeSelectEl.value);
      const label =
        nextThemeMode === "dark" ? "黑夜模式" : nextThemeMode === "light" ? "白天模式" : "跟随系统";
      updateStatus(settingsStatusEl, `已预览${label}，保存后持久化。`, "info");
    });
  }
}
