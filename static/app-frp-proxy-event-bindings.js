// Proxy and FRP event bindings for the home page.
// Called by app.js after managers and DOM globals are initialized.

function bindFrpProxyEventHandlers() {
  proxyTestModeInputEls.forEach((input) => {
    input.addEventListener("change", () => {
      if (input.checked) {
        syncProxyTestModeUi();
      }
    });
  });

  proxyTestBtnEl.addEventListener("click", testProxyFromForm);
  proxySaveBtnEl.addEventListener("click", saveProxyPreset);
  proxyClearActiveBtnEl.addEventListener("click", clearActiveAppProxy);
  proxyClearBtnEl.addEventListener("click", () => {
    clearProxyForm();
    showProxyResult('', 'muted');
    proxyTestResultEl.hidden = true;
  });
  syncProxyTestModeUi();
  if (frpRoleRefreshBtnEl) {
    frpRoleRefreshBtnEl.addEventListener("click", () => {
      setActiveFrpRoleTab("frpc");
      loadFrpRoles();
      loadFrpSystemItems();
    });
  }
  if (frpSystemRefreshBtnEl) {
    frpSystemRefreshBtnEl.addEventListener("click", () => {
      setActiveFrpRoleTab("frpc");
      setFrpCreateSourceMode("frpc", "system");
      loadFrpSystemItems();
    });
  }
  if (frpServerRoleRefreshBtnEl) {
    frpServerRoleRefreshBtnEl.addEventListener("click", () => {
      setActiveFrpRoleTab("frps");
      loadFrpRoles();
      loadFrpSystemItems();
    });
  }
  if (frpServerSystemRefreshBtnEl) {
    frpServerSystemRefreshBtnEl.addEventListener("click", () => {
      setActiveFrpRoleTab("frps");
      setFrpCreateSourceMode("frps", "system");
      loadFrpSystemItems();
    });
  }
  if (frpRoleNewFrpcBtnEl) {
    frpRoleNewFrpcBtnEl.addEventListener("click", () => {
      setActiveFrpRoleTab("frpc");
      fillFrpRoleForm(defaultFrpRole("frpc"));
    });
  }
  if (frpRoleNewFrpsBtnEl) {
    frpRoleNewFrpsBtnEl.addEventListener("click", () => {
      setActiveFrpRoleTab("frps");
      fillFrpRoleForm(defaultFrpRole("frps"));
    });
  }
  if (frpSourceComponentInputEl) {
    frpSourceComponentInputEl.addEventListener("change", () => {
      setActiveFrpRoleTab("frpc");
    });
  }
  if (frpSourceModeInputEl) {
    frpSourceModeInputEl.addEventListener("change", () => syncFrpCreateSourceModeUi("frpc"));
  }
  if (frpServerSourceModeInputEl) {
    frpServerSourceModeInputEl.addEventListener("change", () => syncFrpCreateSourceModeUi("frps"));
  }
  if (frpSourceSystemSelectEl) {
    frpSourceSystemSelectEl.addEventListener("change", () => {
      const item = frpSystemItemById(frpSourceSystemSelectEl.value || "");
      if (!item) return;
      setFrpCreateSourceMode("frpc", "system");
      if (frpSourceComponentInputEl) frpSourceComponentInputEl.value = "frpc";
    });
  }
  if (frpServerSourceSystemSelectEl) {
    frpServerSourceSystemSelectEl.addEventListener("change", () => {
      const item = frpSystemItemById(frpServerSourceSystemSelectEl.value || "");
      if (!item) return;
      setFrpCreateSourceMode("frps", "system");
      if (frpServerSourcePublicPortInputEl) frpServerSourcePublicPortInputEl.value = "7000";
    });
  }
  if (frpSourceTestBtnEl) {
    frpSourceTestBtnEl.addEventListener("click", () => {
      testFrpPublicPort(frpSourcePublicAddrInputEl?.value || "", frpSourcePublicPortInputEl?.value || 0, frpSourceStatusEl);
    });
  }
  if (frpServerSourceTestBtnEl) {
    frpServerSourceTestBtnEl.addEventListener("click", () => {
      testFrpPublicPort(frpServerSourcePublicAddrInputEl?.value || "", frpServerSourcePublicPortInputEl?.value || 0, frpServerSourceStatusEl);
    });
  }
  if (frpSourceAddBtnEl) {
    frpSourceAddBtnEl.addEventListener("click", addFrpSourceToManaged);
  }
  if (frpSourceAdoptSelectedBtnEl) {
    frpSourceAdoptSelectedBtnEl.addEventListener("click", () => {
      setFrpCreateSourceMode("frpc", "system");
      addFrpSourceToManaged();
    });
  }
  if (frpServerSourceAddBtnEl) {
    frpServerSourceAddBtnEl.addEventListener("click", addFrpServerSourceToManaged);
  }
  if (frpServerSourceAdoptSelectedBtnEl) {
    frpServerSourceAdoptSelectedBtnEl.addEventListener("click", () => {
      setFrpCreateSourceMode("frps", "system");
      addFrpServerSourceToManaged();
    });
  }
  function handleFrpRoleTableClick(event) {
    const button = event.target.closest("[data-frp-role-action]");
    const row = event.target.closest("[data-frp-role-id]");
    if (!row) return;
    const id = row?.dataset.frpRoleId || "";
    const status = frpRoleStatusById(id);
    if (!button) {
      fillFrpRoleForm(status);
      return;
    }
    const action = button.dataset.frpRoleAction;
    if (action === "edit") {
      fillFrpRoleForm(status);
    } else if (action === "test") {
      testFrpRolePublicPort(id);
    } else if (action === "start") {
      state.editingFrpRoleId = id;
      setActiveFrpRoleTab(status?.role?.component || state.activeFrpRoleTab);
      runFrpRoleCommand("start", "正在启动 FRP 角色…", "FRP 角色已启动。", id);
    } else if (action === "stop") {
      state.editingFrpRoleId = id;
      setActiveFrpRoleTab(status?.role?.component || state.activeFrpRoleTab);
      runFrpRoleCommand("stop", "正在停止 FRP 角色…", "FRP 角色已停止。", id);
    }
  }
  if (frpServerRoleTableBodyEl) {
    frpServerRoleTableBodyEl.addEventListener("click", handleFrpRoleTableClick);
  }
  if (frpClientRoleTableBodyEl) {
    frpClientRoleTableBodyEl.addEventListener("click", handleFrpRoleTableClick);
  }
  if (frpSystemTableBodyEl) {
    frpSystemTableBodyEl.addEventListener("click", (event) => {
      const button = event.target.closest("[data-frp-system-action]");
      if (!button) return;
      const row = button.closest("[data-frp-system-id]");
      const item = frpSystemItemById(row?.dataset.frpSystemId || "");
      if (button.dataset.frpSystemAction === "unmanage") {
        unmanageFrpSystemItem(item);
      } else {
        adoptFrpSystemItem(item);
      }
    });
  }
  if (frpServerSystemTableBodyEl) {
    frpServerSystemTableBodyEl.addEventListener("click", (event) => {
      const button = event.target.closest("[data-frp-system-action]");
      if (!button) return;
      const row = button.closest("[data-frp-system-id]");
      const item = frpSystemItemById(row?.dataset.frpSystemId || "");
      if (button.dataset.frpSystemAction === "unmanage") {
        unmanageFrpSystemItem(item);
      } else {
        adoptFrpSystemItem(item);
      }
    });
  }
  if (frpRoleComponentInputEl) {
    frpRoleComponentInputEl.addEventListener("change", syncFrpRoleComponentUi);
  }
  if (frpRoleFrpcProxyTypeInputEl) {
    frpRoleFrpcProxyTypeInputEl.addEventListener("change", syncFrpRoleComponentUi);
  }
  if (frpRoleFrpcProxyTableBodyEl) {
    frpRoleFrpcProxyTableBodyEl.addEventListener("click", (event) => {
      const checkbox = event.target.closest("[data-select-frp-proxy]");
      const row = event.target.closest("[data-frp-proxy-index]");
      if (!row) return;
      const index = Number(row.dataset.frpProxyIndex);
      if (!Number.isInteger(index)) return;
      if (checkbox) {
        if (checkbox.checked) state.selectedFrpProxyIndexes.add(index);
        else state.selectedFrpProxyIndexes.delete(index);
        renderFrpProxyRows();
        return;
      }
      state.selectedFrpProxyIndexes.clear();
      state.selectedFrpProxyIndexes.add(index);
      renderFrpProxyRows();
      fillFrpProxyEditor(state.frpRoleDraftProxies[index], index);
    });
  }
  if (frpRoleFrpcProxySelectAllEl) {
    frpRoleFrpcProxySelectAllEl.addEventListener("change", () => {
      state.selectedFrpProxyIndexes.clear();
      if (frpRoleFrpcProxySelectAllEl.checked) {
        state.frpRoleDraftProxies.forEach((_, index) => state.selectedFrpProxyIndexes.add(index));
      }
      renderFrpProxyRows();
    });
  }
  if (frpRoleFrpcProxyAddBtnEl) {
    frpRoleFrpcProxyAddBtnEl.addEventListener("click", () => {
      state.selectedFrpProxyIndexes.clear();
      renderFrpProxyRows();
      fillFrpProxyEditor(defaultFrpProxyConfig(), -1);
      setInlineStatus(frpRoleFrpcProxyStatusEl, "正在新增节点。", "muted");
    });
  }
  if (frpRoleFrpcProxyEditSelectedBtnEl) {
    frpRoleFrpcProxyEditSelectedBtnEl.addEventListener("click", editSelectedFrpProxy);
  }
  if (frpRoleFrpcProxyDuplicateSelectedBtnEl) {
    frpRoleFrpcProxyDuplicateSelectedBtnEl.addEventListener("click", duplicateSelectedFrpProxy);
  }
  if (frpRoleFrpcProxyDeleteSelectedBtnEl) {
    frpRoleFrpcProxyDeleteSelectedBtnEl.addEventListener("click", deleteSelectedFrpProxies);
  }
  if (frpRoleFrpcProxySaveBtnEl) {
    frpRoleFrpcProxySaveBtnEl.addEventListener("click", saveFrpProxyFromEditor);
  }
  if (frpRoleFrpcProxyCancelBtnEl) {
    frpRoleFrpcProxyCancelBtnEl.addEventListener("click", () => {
      setFrpProxyEditorVisible(false);
      setInlineStatus(frpRoleFrpcProxyStatusEl, "节点编辑已取消。", "muted");
    });
  }
  if (frpRoleSaveBtnEl) {
    frpRoleSaveBtnEl.addEventListener("click", () => saveFrpRole());
  }
  if (frpRoleSaveStartBtnEl) {
    frpRoleSaveStartBtnEl.addEventListener("click", () => saveFrpRole({ start: true }));
  }
  if (frpRoleResetBtnEl) {
    frpRoleResetBtnEl.addEventListener("click", () => {
      const status = selectedFrpRoleStatus();
      fillFrpRoleForm(status || defaultFrpRole(frpRoleComponentInputEl?.value || "frpc"));
    });
  }
  if (frpRoleDownloadBtnEl) {
    frpRoleDownloadBtnEl.addEventListener("click", downloadSelectedFrpRoleBinary);
  }
  if (frpRoleStartBtnEl) {
    frpRoleStartBtnEl.addEventListener("click", () => runFrpRoleCommand("start", "正在启动 FRP 角色…", "FRP 角色已启动。"));
  }
  if (frpRoleStopBtnEl) {
    frpRoleStopBtnEl.addEventListener("click", () => runFrpRoleCommand("stop", "正在停止 FRP 角色…", "FRP 角色已停止。"));
  }
  if (frpRoleRestartBtnEl) {
    frpRoleRestartBtnEl.addEventListener("click", () => runFrpRoleCommand("restart", "正在重启 FRP 角色…", "FRP 角色已重启。"));
  }
  if (frpRoleDeleteBtnEl) {
    frpRoleDeleteBtnEl.addEventListener("click", deleteSelectedFrpRole);
  }
  if (frpRoleCloseBtnEl) {
    frpRoleCloseBtnEl.addEventListener("click", () => setFrpRoleEditorVisible(false));
  }
  if (frpcProxyTypeInputEl) {
    frpcProxyTypeInputEl.addEventListener("change", syncFrpcProxyTypeUi);
  }
  if (frpcRefreshBtnEl) {
    frpcRefreshBtnEl.addEventListener("click", loadFrpcStatus);
  }
  if (frpcDownloadBtnEl) {
    frpcDownloadBtnEl.addEventListener("click", downloadFrpcBinary);
  }
  if (frpcSaveBtnEl) {
    frpcSaveBtnEl.addEventListener("click", () => saveFrpcConfig());
  }
  if (frpcSaveStartBtnEl) {
    frpcSaveStartBtnEl.addEventListener("click", () => saveFrpcConfig({ start: true }));
  }
  if (frpcStartBtnEl) {
    frpcStartBtnEl.addEventListener("click", () => runFrpcCommand("start", "正在启动 frpc…", "frpc 已启动。"));
  }
  if (frpcStopBtnEl) {
    frpcStopBtnEl.addEventListener("click", () => runFrpcCommand("stop", "正在停止 frpc…", "frpc 已停止。"));
  }
  if (frpcRestartBtnEl) {
    frpcRestartBtnEl.addEventListener("click", () => runFrpcCommand("restart", "正在重启 frpc…", "frpc 已重启。"));
  }
  if (frpsRefreshBtnEl) {
    frpsRefreshBtnEl.addEventListener("click", loadFrpsStatus);
  }
  if (frpsDownloadBtnEl) {
    frpsDownloadBtnEl.addEventListener("click", downloadFrpsBinary);
  }
  if (frpsSaveBtnEl) {
    frpsSaveBtnEl.addEventListener("click", () => saveFrpsConfig());
  }
  if (frpsSaveStartBtnEl) {
    frpsSaveStartBtnEl.addEventListener("click", () => saveFrpsConfig({ start: true }));
  }
  if (frpsStartBtnEl) {
    frpsStartBtnEl.addEventListener("click", () => runFrpsCommand("start", "正在启动 frps…", "frps 已启动。"));
  }
  if (frpsStopBtnEl) {
    frpsStopBtnEl.addEventListener("click", () => runFrpsCommand("stop", "正在停止 frps…", "frps 已停止。"));
  }
  if (frpsRestartBtnEl) {
    frpsRestartBtnEl.addEventListener("click", () => runFrpsCommand("restart", "正在重启 frps…", "frps 已重启。"));
  }
  syncFrpcProxyTypeUi();
}
