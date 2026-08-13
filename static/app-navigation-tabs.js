function currentTabPathname() {
  if (state.activeTab === "workspace-history") {
    return "/workspace_history";
  }
  if (state.activeTab === "terminal-archives") {
    return "/archives";
  }
  if (state.activeTab === "api") {
    return "/codex_api";
  }
  if (state.activeTab === "claude") {
    return "/claude_api";
  }
  if (state.activeTab === "settings") {
    const settingsTab = state.activeSettingsTab || "system";
    return settingsTab === "system" ? "/settings" : `/settings/${settingsTab}`;
  }
  if (state.activeTab === "auth") {
    return "/codex_oauth";
  }
  if (state.activeTab === "desktop") {
    return "/desktop";
  }
  return "/workspace";
}

function workspacePathQuery(path) {
  const params = new URLSearchParams();
  if (path) {
    params.set("path", path);
  }
  if (state.returnTerminalSessionId) {
    params.set("terminal_session", state.returnTerminalSessionId);
  }
  const query = params.toString();
  return query ? `?${query}` : "";
}

// File-browser directory links always target the workspace tab root.
function buildWorkspaceUrl(path) {
  return `/workspace${workspacePathQuery(path)}`;
}

function syncTabUrl() {
  const nextUrl = `${currentTabPathname()}${workspacePathQuery(state.currentPath)}`;
  window.history.replaceState({}, "", nextUrl);
}

function setTabPanelActive(panel, active) {
  if (!panel) {
    return;
  }
  panel.classList.toggle("active", active);
  panel.hidden = !active;
  panel.setAttribute("aria-hidden", active ? "false" : "true");
  if (active) {
    refreshBackdropFilter(panel);
  }
}

// Panels use backdrop-filter:blur(...), whose result the browser caches in a
// compositor layer. When a panel is re-shown (display:none -> grid via
// [hidden]), that cached layer can briefly flash stale content captured under
// a different theme (the "ghost" / residual image). Dropping backdrop-filter
// to "none" for one frame forces the compositor to discard the stale layer;
// restoring it on the next frame re-captures the current background.
function refreshBackdropFilter(panel) {
  if (!panel) return;
  const computed = window.getComputedStyle(panel);
  if (!computed.backdropFilter || computed.backdropFilter === "none") return;
  panel.classList.add("backdrop-refresh");
  void panel.offsetHeight; // flush style so the class takes effect this frame
  requestAnimationFrame(() => {
    panel.classList.remove("backdrop-refresh");
  });
}

function setActiveSettingsTab(tab) {
  tab = normalizeSettingsTab(tab);
  if (state.activeSettingsTab === "auto-continue-tasks" && tab !== "auto-continue-tasks") {
    // Leaving the task panel: stop the live countdown ticker.
    stopPasteScheduledTaskTicker();
  }
  state.activeSettingsTab = tab;
  syncSettingsCategoryNavigation(tab);
  settingsSubpanels.forEach((panel) => {
    setTabPanelActive(panel, panel.dataset.settingsPanel === tab);
  });
  if (tab === "proxy") {
    loadProxyPresets();
    systemPanelManager.loadSystemProxyStatus();
  }
  if (tab === "compile") {
    compileStatusManager.startLiveRefresh();
  } else {
    compileStatusManager.stopLiveRefresh();
  }
  if (tab === "auto-continue-tasks") {
    loadUnifiedTerminals();
    loadUnifiedPresetTargets();
    loadUnifiedTasks();
  }
  if (tab === "frpc" || tab === "frps") {
    setActiveFrpRoleTab(tab === "frps" ? "frps" : "frpc");
    loadFrpRoles();
    loadFrpSystemItems();
  }
  if (tab === "preset-sync") {
    presetSyncManager.setPresetSyncBusy(false);
  }
  if (tab === "config-files" && !state.settingsConfigFileLoaded) {
    loadSettingsConfigFile();
  }
  if (state.activeTab === "settings") {
    syncTabUrl();
  }
}

function markWorkspaceHistorySettingsReady() {
  const wasReady = state.workspaceHistorySettingsReady;
  state.workspaceHistorySettingsReady = true;
  if (!wasReady && state.activeTab === "workspace-history") {
    renderWorkspaceHistory();
    refreshWorkspaceHistoryConversations();
  }
}

function setActiveTab(tab) {
  if (tab === "auth-inspect") {
    tab = "auth";
  }
  if (tab === "sessions") {
    tab = "workspace";
  }
  const enteringWorkspaceHistory = tab === "workspace-history" && state.activeTab !== tab;
  state.activeTab = tab;
  tabButtons.forEach((button) => {
    const active = button.dataset.tab === tab;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  });
  setTabPanelActive(workspaceViewEl, tab === "workspace");
  setTabPanelActive(workspaceHistoryViewEl, tab === "workspace-history");
  setTabPanelActive(terminalArchivesViewEl, tab === "terminal-archives");
  setTabPanelActive(authViewEl, tab === "auth");
  setTabPanelActive(apiViewEl, tab === "api");
  setTabPanelActive(claudeViewEl, tab === "claude");
  setTabPanelActive(settingsViewEl, tab === "settings");
  setTabPanelActive(desktopViewEl, tab === "desktop");
  if (tab === "settings") {
    setActiveSettingsTab(state.activeSettingsTab);
    window.requestAnimationFrame(() => {
      workspaceDirInputEl.scrollLeft = 0;
    });
  }
  if (tab === "workspace") {
    loadSessions();
  }
  if (tab === "terminal-archives") {
    loadTerminalArchives();
  }
  if (tab === "workspace-history") {
    if (enteringWorkspaceHistory) {
      state.workspaceHistorySelectedPath = state.currentWorkspaceDirectoryPath || "";
    }
    renderWorkspaceHistory();
    if (enteringWorkspaceHistory) {
      prioritizeWorkspaceHistoryCurrentDirectory();
    }
    if (state.workspaceHistorySettingsReady) {
      const prioritizedPath = enteringWorkspaceHistory
        ? state.workspaceHistorySelectedPath
        : "";
      const refreshRequest = refreshWorkspaceHistoryConversations();
      if (prioritizedPath && refreshRequest && typeof refreshRequest.finally === "function") {
        refreshRequest.finally(() => {
          if (
            state.activeTab === "workspace-history" &&
            state.workspaceHistorySelectedPath === prioritizedPath
          ) {
            prioritizeWorkspaceHistoryCurrentDirectory();
          }
        });
      }
    }
  }
  if (tab === "auth") {
    ensureAuthPresetsLoaded();
  }
  if (tab === "api") {
    ensureApiPresetsLoaded();
    loadCodexCommonConfig();
  }
  if (tab === "claude") {
    ensureClaudePresetsLoaded();
  }
  syncTabUrl();
}
